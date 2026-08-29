import { test, expect } from '@playwright/test'

// notes 动效编辑器:渲染层结构 / 逐字进场 / ghost 离场 / 光标状态机 / IME / 选区。
// 纯浏览器环境(Vite dev server),动效以 class 状态断言(时序窗口内捕获)。
test.describe('notes 记事本', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/')
    await page.waitForSelector('#main-search-input', { timeout: 10000 })
    const input = page.locator('#main-search-input')
    await input.fill('/notes')
    await page.waitForTimeout(200)
    // 扩展入口(kind=extension)回车 → 框架激活 notes
    await input.press('Enter')
    await expect(page.locator('.notes-layer')).toBeVisible({ timeout: 5000 })
  })

  test('空态 placeholder 与光标定位点击', async ({ page }) => {
    await expect(page.locator('.notes-ph')).toBeVisible()
    // 输入区壳:ui-field 面类 + panel 圆角
    await expect(page.locator('.notes-box.ui-field.radius-panel')).toBeVisible()
    // 点击渲染层 → textarea 聚焦 → 光标亮起
    await page.locator('.notes-layer').click()
    await expect(page.locator('.caret-on')).toBeVisible()
    // 光标在内容原点附近(空文本,行首)
    const left = await page.locator('.caret').evaluate((el) => parseFloat(el.style.left))
    expect(left).toBeLessThan(2)
  })

  test('逐字输入:字符渲染 + 进场动画落地清理 + 光标右移', async ({ page }) => {
    await page.locator('.notes-layer').click()
    await page.keyboard.type('hi')
    const chars = page.locator('.notes-layer .ch')
    await expect(chars).toHaveCount(2)
    // 动画窗口内捕获进场标记;每字带伪随机微旋转注入(--rot,打破同质落定)
    expect(await page.locator('.ch.anim').count()).toBeGreaterThan(0)
    const rot = await page
      .locator('.ch.anim')
      .first()
      .evaluate((el) => el.style.getPropertyValue('--rot'))
    expect(rot).toMatch(/^-?\d+(\.\d+)?deg$/)
    // 动画结束清理(inline-block → inline 断词回归)
    await expect(page.locator('.ch.anim')).toHaveCount(0, { timeout: 3000 })
    // 光标随输入右移
    const left = await page.locator('.caret').evaluate((el) => parseFloat(el.style.left))
    expect(left).toBeGreaterThan(4)
    // IME 锚:textarea 壳跟随自绘光标位置(候选栏定位依据),内部滚动同步拉回 caret
    const anchor = await page.evaluate(() => {
      const ta = document.querySelector('.notes-input') as HTMLTextAreaElement
      const layer = document.querySelector('.notes-layer') as HTMLElement
      const caret = document.querySelector('.caret') as HTMLElement
      return {
        taLeft: parseFloat(ta.style.left),
        taScrollLeft: ta.scrollLeft,
        layerLeft: layer.offsetLeft,
        caretLeft: parseFloat(caret.style.left),
      }
    })
    expect(Math.abs(anchor.taLeft - (anchor.layerLeft + anchor.caretLeft))).toBeLessThan(1)
    expect(Math.abs(anchor.taScrollLeft - anchor.caretLeft)).toBeLessThanOrEqual(1)
  })

  test('光标垂直稳定:动画态与稳定态字符间测量不跳动', async ({ page }) => {
    await page.locator('.notes-layer').click()
    // 首批输入:anchor 字符处于 inline-block 动画态(rect.top = 行顶)
    await page.keyboard.type('abc')
    await expect(page.locator('.ch')).toHaveCount(3)
    const top1 = await page.locator('.caret').evaluate((el) => el.style.top)
    // 等全部字符动画结束转 inline(rect.top = 字体内容盒顶,基线差来源)
    await expect(page.locator('.ch.anim')).toHaveCount(0, { timeout: 3000 })
    // 稳定态继续输入:量化后 top 必须与动画态一致(不垂直跳动、与文字对齐)
    await page.keyboard.type('def')
    const top2 = await page.locator('.caret').evaluate((el) => el.style.top)
    expect(top1).toBe('3px')
    expect(top2).toBe('3px')
  })

  test('光标移动 Q 弹:三段 squash-stretch 形变,到位后清空', async ({ page }) => {
    await page.locator('.notes-layer').click()
    await page.keyboard.type('ab')
    // 移动瞬间 caret-core 存在 WAAPI 形变动画(排除 CSS 闪烁),且为三段形变(拉伸→反弹→回正),
    // 首帧幅度 ≥1.25 保证逐字输入也可感知(2px 宽光标的微幅 scale 不可见)
    const frames = await page.locator('.caret-core').evaluate((el) =>
      el
        .getAnimations()
        .filter((a) => !(a instanceof CSSAnimation))
        .flatMap((a) => a.effect?.getKeyframes?.() ?? []),
    )
    // 三段形变:两段 scale(拉伸→反弹)+ 尾帧 none 归正
    expect(frames.length).toBeGreaterThanOrEqual(3)
    const scaleFrames = frames.filter((k) => String(k.transform ?? '').includes('scale'))
    expect(scaleFrames.length).toBeGreaterThanOrEqual(2)
    const m = String(frames[0]?.transform ?? '').match(/scale[XY]\(([\d.]+)\)/)
    expect(m).toBeTruthy()
    expect(Number(m![1])).toBeGreaterThanOrEqual(1.25)
    // 形变结束后仅剩 CSS 闪烁,无 WAAPI 残留
    await page.waitForTimeout(450)
    const rest = await page
      .locator('.caret-core')
      .evaluate((el) => el.getAnimations().filter((a) => !(a instanceof CSSAnimation)))
    expect(rest).toHaveLength(0)
    // 垂直位置不受形变影响(quantified 网格)
    const top = await page.locator('.caret').evaluate((el) => el.style.top)
    expect(top).toBe('3px')
  })

  test('上下键行导航:按渲染层行移动,即时匹配最近列', async ({ page }) => {
    await page.locator('.notes-layer').click()
    await page.keyboard.type('aaa')
    await page.keyboard.press('Enter')
    await page.keyboard.type('bbbbb')
    await expect(page.locator('.ch')).toHaveCount(9, { timeout: 3000 }) // 含换行符
    await expect(page.locator('.ch.anim')).toHaveCount(0, { timeout: 3000 })
    const caret = page.locator('.caret')
    // 末尾:第二行
    expect(await caret.evaluate((el) => el.style.top)).toBe('27px')
    const xBottom = await caret.evaluate((el) => parseFloat(el.style.left))
    // 上:到第一行(当前列超出行宽,落第一行行尾)
    await page.keyboard.press('ArrowUp')
    expect(await caret.evaluate((el) => el.style.top)).toBe('3px')
    const xTop = await caret.evaluate((el) => parseFloat(el.style.left))
    expect(xTop).toBeLessThan(xBottom)
    // 首行再上:文档首
    await page.keyboard.press('ArrowUp')
    expect(await caret.evaluate((el) => parseFloat(el.style.left))).toBeLessThan(1)
    // 回第一行行尾后下:以当前列即时匹配第二行最近位置,不记忆原列
    await page.keyboard.press('Meta+ArrowRight')
    await page.keyboard.press('ArrowDown')
    expect(await caret.evaluate((el) => el.style.top)).toBe('27px')
    const xBack = await caret.evaluate((el) => parseFloat(el.style.left))
    expect(Math.abs(xBack - xTop)).toBeLessThan(5)
    // Cmd+Left/Right:行首/行尾
    await page.keyboard.press('Meta+ArrowLeft')
    expect(await caret.evaluate((el) => parseFloat(el.style.left))).toBeLessThan(1)
    await page.keyboard.press('Meta+ArrowRight')
    const xEnd = await caret.evaluate((el) => parseFloat(el.style.left))
    expect(Math.abs(xEnd - xBottom)).toBeLessThan(1)
  })

  test('中间输入:FLIP 期间换行结构不闪变(换行符不 inline-block 化)', async ({ page }) => {
    await page.locator('.notes-layer').click()
    // 两行文本,光标置于第一行中间
    await page.evaluate(() => {
      const ta = document.querySelector('.notes-input') as HTMLTextAreaElement
      ta.value = 'first line\nsecond line'
      ta.dispatchEvent(new Event('input', { bubbles: true }))
      ta.setSelectionRange(5, 5)
      ta.dispatchEvent(new Event('select', { bubbles: true }))
    })
    await page.waitForTimeout(400)
    await expect(page.locator('.ch.anim')).toHaveCount(0, { timeout: 3000 })
    const secondTop = async (ti: number) =>
      page.evaluate((idx) => {
        const el = document.querySelector(`.notes-layer .ch[data-ti='${idx}']`)
        return el ? el.offsetTop : -1
      }, ti)
    // 'first line\nsecond line':second 行首 s 的 ti=11,插入 X 后为 12
    expect(await secondTop(11)).toBe(28)
    // 中间插入触发后缀 FLIP:第二行在 FLIP 进行中必须保持原行位
    //(\n 被 inline-block 化会使其换行失效,后续文本瞬间并作一行、清理后才恢复)
    await page.keyboard.type('X')
    await page.waitForTimeout(40)
    expect(await secondTop(12)).toBe(28)
    await page.waitForTimeout(400)
    expect(await secondTop(12)).toBe(28)
  })

  test('FLIP 位移垂直分量恒零(视觉盒测量基元)', async ({ page }) => {
    await page.locator('.notes-layer').click()
    await page.keyboard.type('abcdef')
    await expect(page.locator('.ch.anim')).toHaveCount(0, { timeout: 3000 })
    // 字形视觉盒用 Range 测(裸 gBCR 随盒语义漂移:inline 给字形盒、inline-block 给边框盒)
    const charTop = (ti: number) =>
      page.evaluate((idx) => {
        const el = document.querySelector(`.notes-layer .ch[data-ti='${idx}']`) as HTMLElement
        if (!el) return NaN
        const r = document.createRange()
        r.selectNodeContents(el)
        return r.getBoundingClientRect().top
      }, ti)
    const before = await charTop(5)
    // 光标移到中间删 'd',后缀 e/f FLIP 滑入
    await page.keyboard.press('ArrowLeft')
    await page.keyboard.press('ArrowLeft')
    await page.keyboard.press('ArrowLeft')
    await page.keyboard.press('Backspace')
    await page.waitForTimeout(40)
    // FLIP 进行中(gBCR 含 transform):后缀字符垂直位置必须与静止时一致
    // (若按 offsetTop 盒语义测量,inline→inline-block 切换引入 4px 垂直漂移)
    expect(await charTop(4)).toBe(before)
    await page.waitForTimeout(400)
    expect(await charTop(4)).toBe(before)
  })

  test('连续删除无幽灵字符(ghost 飘散中再 diff)', async ({ page }) => {
    await page.locator('.notes-layer').click()
    await page.keyboard.type('abcd')
    // 快速连续删除:第一次的 ghost 还在飘散(210ms 窗口)时第二次 diff
    // 不得错抓 ghost 致真实被删字符逃逸为不可编辑的幽灵
    await page.keyboard.press('Backspace')
    await page.waitForTimeout(60)
    await page.keyboard.press('Backspace')
    await expect(page.locator('.ch')).toHaveCount(2, { timeout: 3000 })
    const ghostly = await page.locator('.ch:not(.ghost):not([data-ti])').count()
    expect(ghostly).toBe(0)
    const value = await page
      .locator('.notes-input')
      .evaluate((el) => (el as HTMLTextAreaElement).value)
    expect(value).toBe('ab')
  })

  test('中间插入光标与内容同位(FLIP 进行中测量不被污染)', async ({ page }) => {
    await page.locator('.notes-layer').click()
    await page.keyboard.type('abcd')
    await expect(page.locator('.ch.anim')).toHaveCount(0, { timeout: 3000 })
    await page.keyboard.press('ArrowLeft')
    await page.keyboard.press('ArrowLeft')
    await page.keyboard.type('X') // abXcd,光标应在 X 后 = 'c'(ti=3)左缘
    await page.waitForTimeout(30) // FLIP 滑移进行中
    const aligned = await page.evaluate(() => {
      const layer = document.querySelector('.notes-layer') as HTMLElement
      const caret = document.querySelector('.caret') as HTMLElement
      const c = layer.querySelector(".ch[data-ti='3']") as HTMLElement
      return Math.abs(parseFloat(caret.style.left) - c.offsetLeft) < 1
    })
    expect(aligned).toBe(true)
  })

  test('删除:ghost 离场 + 字符回收', async ({ page }) => {
    await page.locator('.notes-layer').click()
    await page.keyboard.type('ab')
    await expect(page.locator('.ch')).toHaveCount(2)
    await page.keyboard.press('Backspace')
    // ghost 窗口内捕获离场标记;行盒高须取被删字符内容盒实测(--gh 注入,
    // 硬编码行高在 WKWebView 下因字体 metrics 差异把字形顶出行盒,呈整体偏上)
    const ghost = page.locator('.ch.ghost').first()
    await expect(ghost).toBeVisible({ timeout: 1000 })
    const ghostLh = await ghost.evaluate((el) => parseFloat(getComputedStyle(el).lineHeight))
    expect(ghostLh).toBeGreaterThan(0)
    expect(ghostLh).toBeLessThan(20)
    // ghost 到期回收,存活字符归位
    await expect(page.locator('.ch')).toHaveCount(1, { timeout: 3000 })
    await expect(page.locator('.ch.ghost')).toHaveCount(0)
  })

  test('IME 拼音全程动画:字母逐个弹入,提交时汉字弹入', async ({ page }) => {
    await page.locator('.notes-layer').click()
    const type = (v: string) =>
      page.evaluate((val) => {
        const ta = document.querySelector('.notes-input') as HTMLTextAreaElement
        ta.value = val
        ta.dispatchEvent(new Event('input', { bubbles: true }))
      }, v)
    // 拼音组合阶段:字母逐个输入,每个都带进场动画(组合下划线)
    await page.evaluate(() => {
      const ta = document.querySelector('.notes-input') as HTMLTextAreaElement
      ta.dispatchEvent(new CompositionEvent('compositionstart'))
    })
    await type('n')
    expect(await page.locator('.ch.anim').count()).toBe(1)
    await type('ni')
    await expect(page.locator('.ch')).toHaveCount(2)
    await type('nihao')
    await expect(page.locator('.ch')).toHaveCount(5)
    expect(await page.locator('.ch.comp').count()).toBe(5)
    // 提交:字母段整体替换为汉字(先 input 后 compositionend,WebKit 时序)
    await type('你好')
    // 汉字弹入动画(200ms 窗口内立即断言;字母 ghost 飘散中)
    expect(await page.locator('.ch.anim').count()).toBe(2)
    await page.evaluate(() => {
      const ta = document.querySelector('.notes-input') as HTMLTextAreaElement
      ta.dispatchEvent(new CompositionEvent('compositionend'))
    })
    await expect(page.locator('.ch')).toHaveCount(2, { timeout: 3000 })
    await expect(page.locator('.ch.comp')).toHaveCount(0, { timeout: 3000 })
    await expect(page.locator('.ch.anim')).toHaveCount(0, { timeout: 3000 })
    const text = await page.locator('.notes-layer').evaluate((el) => el.textContent)
    expect(text).toContain('你好')
  })

  test('全选:选区高亮 + 光标隐藏,取消选区恢复', async ({ page }) => {
    await page.locator('.notes-layer').click()
    await page.keyboard.type('sel')
    await expect(page.locator('.ch')).toHaveCount(3)
    await page.keyboard.press('Meta+a')
    await expect(page.locator('.ch.sel')).toHaveCount(3)
    // 选区存在时光标隐藏
    await expect(page.locator('.caret-on')).toHaveCount(0)
    await page.keyboard.press('ArrowRight')
    await expect(page.locator('.ch.sel')).toHaveCount(0)
    await expect(page.locator('.caret-on')).toBeVisible()
  })

  test('点击定位:点击行中偏右处光标落到字符间', async ({ page }) => {
    await page.locator('.notes-layer').click()
    await page.keyboard.type('abcdef')
    await expect(page.locator('.ch')).toHaveCount(6)
    await expect(page.locator('.ch.anim')).toHaveCount(0, { timeout: 3000 })
    // 点击渲染层中部 → 光标 cp 偏移 > 0 且 < 6
    const box = await page.locator('.notes-layer').boundingBox()
    expect(box).toBeTruthy()
    await page.mouse.click(box!.x + box!.width / 2, box!.y + 8)
    const left = await page.locator('.caret').evaluate((el) => parseFloat(el.style.left))
    expect(left).toBeGreaterThan(4)
    expect(left).toBeLessThan(60)
    // 光标处继续输入落在中部而非末尾
    await page.keyboard.type('X')
    const text = await page.locator('.notes-layer').evaluate((el) => el.textContent)
    expect(text).toMatch(/^abcX?d/)
  })

  test('设置子视图:入口与快捷键配置项', async ({ page }) => {
    // 搜索栏右侧设置按钮 → config 子视图
    await page.locator('button:has(.i-ri-settings-3-line)').click()
    await page.waitForTimeout(200)
    await expect(page.locator('.notes-layer')).toHaveCount(0) // mainView 让位
    // BaseSettingsList 渲染:快捷键项 + 清空项
    await expect(page.getByText('启动快捷键')).toBeVisible({ timeout: 5000 })
    await expect(page.getByText('清空内容')).toBeVisible()
    // 再点设置按钮(激活态 fill 图标)返回正文视图
    await page.locator('button:has(.i-ri-settings-3-fill)').click()
    await page.waitForTimeout(200)
    await expect(page.locator('.notes-layer')).toBeVisible({ timeout: 5000 })
  })

  test('长文滚动跟随:光标行保持在可视区', async ({ page }) => {
    await page.locator('.notes-layer').click()
    await page.evaluate(() => {
      const ta = document.querySelector('.notes-input') as HTMLTextAreaElement
      ta.value = Array.from({ length: 40 }, (_, i) => `line${i}`).join('\n')
      ta.dispatchEvent(new Event('input', { bubbles: true }))
    })
    await page.waitForTimeout(500)
    // 光标跳到文末(Cmd+Down 原生文档尾导航):滚动容器须跟随滚到光标行
    await page.keyboard.press('Meta+ArrowDown')
    await page.waitForTimeout(100)
    const state = await page.evaluate(() => {
      const scroller = document.querySelector('.overflow-y-auto') as HTMLElement
      const caret = document.querySelector('.caret') as HTMLElement
      const cr = caret.getBoundingClientRect()
      const sr = scroller.getBoundingClientRect()
      return {
        scrollTop: scroller.scrollTop,
        inView: cr.bottom <= sr.bottom - 8 && cr.top >= sr.top + 76,
      }
    })
    expect(state.scrollTop).toBeGreaterThan(0)
    expect(state.inView).toBe(true)
  })

  test('长文本降级:超阈值仍有渲染与光标', async ({ page }) => {
    await page.locator('.notes-layer').click()
    await page.evaluate(() => {
      const ta = document.querySelector('.notes-input') as HTMLTextAreaElement
      ta.value = 'x'.repeat(1500)
      ta.dispatchEvent(new Event('input', { bubbles: true }))
    })
    await expect(page.locator('.ch')).toHaveCount(1500)
    // 降级路径无进场动画标记
    await expect(page.locator('.ch.anim')).toHaveCount(0)
    await expect(page.locator('.caret-on')).toBeVisible()
  })
})
