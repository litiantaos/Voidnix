import { test, expect, type Page } from '@playwright/test'
import type { Pinia } from 'pinia'

// screenshot OCR：预览区（长图可滚 + 加载遮罩钉住视口）与操作按钮行（左右键切换、
// 回车触发）。纯浏览器环境动态 import 扩展模块直写 ocrSession，真实布局/键盘验证。

/// 浏览器直驱扩展模块：写 ocrSession + 激活 screenshot 的 ocr subview。
/// 动态 import 必须命中页面模块图的同一实例：dev server 经 HMR 后模块 URL 带
/// ?t= 版本参数，裸路径 import 会创建新模块记录，顶层 defineExtension 二次执行
/// 即 duplicate 报错——从 resource timing 取页面实际加载的 URL（含 ?t=，取最新）
async function openOcrView(
  page: Page,
  session: { imageUrl: string; ocrText?: string; error?: string; loading?: boolean },
) {
  await page.evaluate(async (s) => {
    const el = document.querySelector('#app') as HTMLElement & {
      __vue_app__?: { config: { globalProperties: { $pinia?: Pinia } } }
    }
    const pinia = el.__vue_app__?.config?.globalProperties?.$pinia
    if (!pinia) throw new Error('pinia not exposed')
    const latestModuleUrl = (file: string) => {
      const hits = performance
        .getEntriesByType('resource')
        .map((e) => e.name)
        .filter((n) => n.includes(file))
      return hits[hits.length - 1] ?? file
    }
    const { useAppStore } = await import(latestModuleUrl('/src/stores/app.ts'))
    const { ocrSession } = await import(latestModuleUrl('/extensions/screenshot/index.ts'))
    Object.assign(ocrSession.value, s)
    const store = useAppStore(pinia)
    store.setActiveExtension('screenshot')
    store.openSubview('ocr')
  }, session)
}

/// 改 ocrSession.loading(遮罩离场等)。模块 URL 查找同 openOcrView:HMR 后带 ?t=,
/// 裸路径 import 创建新模块记录会触发 defineExtension duplicate
async function setOcrLoading(page: Page, loading: boolean) {
  await page.evaluate(async (v) => {
    const hits = performance
      .getEntriesByType('resource')
      .map((e) => e.name)
      .filter((n) => n.includes('/extensions/screenshot/index.ts'))
    const { ocrSession } = await import(hits[hits.length - 1] ?? '/extensions/screenshot/index.ts')
    ocrSession.value.loading = v
  }, loading)
}

test.describe('screenshot OCR', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/')
    await page.waitForSelector('#main-search-input', { timeout: 10000 })
  })

  test('加载遮罩不随长图滚动：钉住预览视口，识别完成即消失', async ({ page }) => {
    const longImage =
      "data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' width='600' height='3000'><rect width='600' height='3000' fill='%23226644'/></svg>"

    await openOcrView(page, { imageUrl: longImage, loading: true })

    // 等图片 load：onPreviewLoad 按精确 cover 尺寸写内联宽高（切可滚动模式）
    const img = page.locator('.hide-scrollbar img')
    await expect(img).toBeVisible({ timeout: 5000 })
    await expect.poll(async () => img.evaluate((el) => el.style.height)).not.toBe('')

    const measure = () => {
      // hide-scrollbar 全局多处复用：从预览图向上精确定位滚动层，
      // 遮罩取滚动层的兄弟（预览容器直接子级）；覆盖参照取滚动层视口
      // （用户实际可见区，不含外层 1px border）
      const img = document.querySelector<HTMLElement>('.hide-scrollbar img')!
      const scroll = img.closest<HTMLElement>('.hide-scrollbar')!
      const mask = scroll.parentElement!.querySelector<HTMLElement>(':scope > .backdrop-blur-xs')!
      const frame = scroll.getBoundingClientRect()
      const r = mask.getBoundingClientRect()
      return {
        scrollable: scroll.scrollHeight - scroll.clientHeight,
        scrollTop: scroll.scrollTop,
        maskTop: r.top,
        coverX: Math.abs(r.left - frame.left) < 0.5 && Math.abs(r.right - frame.right) < 0.5,
        coverY: Math.abs(r.top - frame.top) < 0.5 && Math.abs(r.bottom - frame.bottom) < 0.5,
      }
    }

    const before = await page.evaluate(measure)
    // 长图确实溢出（3000 级高度 vs 176 容器）
    expect(before.scrollable).toBeGreaterThan(500)

    // 滚动内层：旧实现（遮罩在滚动容器内）遮罩随内容上移出视口
    await page.evaluate(() => {
      const scroll = document
        .querySelector<HTMLElement>('.hide-scrollbar img')!
        .closest<HTMLElement>('.hide-scrollbar')!
      scroll.scrollTop = 800
    })
    const after = await page.evaluate(measure)
    expect(after.scrollTop).toBe(800) // 滚动确实生效
    expect(after.coverY).toBe(true) // 遮罩仍覆盖视口
    expect(after.coverX).toBe(true)
    expect(Math.abs(after.maskTop - before.maskTop)).toBeLessThan(1) // 钉住不动

    // 识别完成：遮罩离场消失
    await setOcrLoading(page, false)
    await expect(page.locator('.backdrop-blur-xs')).toHaveCount(0, { timeout: 3000 })
  })

  test('操作按钮行：左右键切换，回车触发；textarea 聚焦时让出', async ({ page }) => {
    const tinyImage =
      "data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' width='40' height='30'><rect width='40' height='30' fill='%23336699'/></svg>"
    await openOcrView(page, { imageUrl: tinyImage, ocrText: 'hello  world' })

    // 挂载即聚焦按钮导航（disableSearchInput 扩展不自动聚焦 textarea）
    const btn = (name: string) => page.getByRole('button', { name, exact: true })
    await expect(btn('复制')).toBeVisible({ timeout: 5000 })
    await expect(btn('复制')).toHaveClass(/ui-active/)
    expect(await page.locator('.backdrop-blur-xs').count()).toBe(0)

    // 圆角裁剪：外层 overflow-hidden + radius-panel（10px）把图片/遮罩四角裁进
    // 圆角（修复前直角图片溢出圆角边框范围）。elementFromPoint 不跟随圆角裁剪
    // （命中区仍是矩形），改截图像素采样：圆角外角点为页面背景色、圆角内为图片色
    const frame = await page.evaluate(() => {
      const img = document.querySelector<HTMLElement>('.hide-scrollbar img')!
      const outer = img.closest<HTMLElement>('.hide-scrollbar')!.parentElement!
      const cs = getComputedStyle(outer)
      const r = outer.getBoundingClientRect()
      return { overflow: cs.overflow, radius: cs.borderRadius, x: r.left, y: r.top }
    })
    expect(frame.overflow).toBe('hidden')
    expect(frame.radius).toBe('10px')
    const shot = await page.screenshot({
      clip: { x: frame.x, y: frame.y, width: 14, height: 14 },
    })
    const px = await page.evaluate(async (b64) => {
      const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0))
      const bmp = await createImageBitmap(new Blob([bytes]))
      const canvas = document.createElement('canvas')
      canvas.width = bmp.width
      canvas.height = bmp.height
      const ctx = canvas.getContext('2d')!
      ctx.drawImage(bmp, 0, 0)
      const corner = ctx.getImageData(0, 0, 1, 1).data
      const inner = ctx.getImageData(bmp.width - 1, bmp.height - 1, 1, 1).data
      return {
        corner: [corner[0], corner[1], corner[2]],
        inner: [inner[0], inner[1], inner[2]],
      }
    }, shot.toString('base64'))
    expect(px.corner).not.toEqual([51, 102, 153]) // #336699，未被裁剪时角点即图片色
    expect(px.inner).toEqual([51, 102, 153])

    // 切换选中零跳动：所有按钮几何在 active 切换前后恒定（active 摘 soft-chip 会
    // 丢 1px 边框致内容盒 ±2px、同行按钮平移）
    const boxOf = async () =>
      Promise.all(
        ['复制', '翻译', '去空格', '去换行', '去空行'].map(async (n) => {
          const b = await btn(n).boundingBox()
          return b ? [b.x, b.y, b.width, b.height] : null
        }),
      )
    const boxesBefore = await boxOf()
    await page.keyboard.press('ArrowRight')
    await expect(btn('翻译')).toHaveClass(/ui-active/)
    expect(await boxOf()).toEqual(boxesBefore)

    // 右移一次选中「去空格」，回车触发（真实键盘）
    await page.keyboard.press('ArrowRight')
    await expect(btn('去空格')).toHaveClass(/ui-active/)
    await page.keyboard.press('Enter')
    await expect(page.locator('textarea')).toHaveValue('helloworld')

    // textarea 聚焦（点击进入编辑）：左右键移动光标、回车换行，不切换不触发
    await page.locator('textarea').click()
    await page.keyboard.press('ArrowRight')
    await expect(btn('去空格')).toHaveClass(/ui-active/)
    await page.keyboard.press('Enter')
    const NEWLINE = String.fromCharCode(10)
    await expect(page.locator('textarea')).toHaveValue('helloworld' + NEWLINE)

    // 点击预览区 blur：恢复按钮导航（左移回到「翻译」；环形边界由单测覆盖）
    await page.locator('.hide-scrollbar img').click()
    await page.keyboard.press('ArrowLeft')
    await expect(btn('翻译')).toHaveClass(/ui-active/)
  })
})
