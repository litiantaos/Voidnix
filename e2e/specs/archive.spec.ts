import { test, expect, type Page } from '@playwright/test'

// archive 扩展布局与交互回归：mock 数据经 CustomEvent 投递进入解压/压缩模式。
//
// 回归锚点 1（布局）：BaseButton 标签必须走默认插槽——`:label` prop 不存在（fallthrough
// 静默忽略），slot 缺席触发 isIconOnly() 的 icon-only chip 分支（w-7 + 无内容），在 auto
// 高度上下文中高度解析失控（实测 384px），把所在行撑到 392px（「纵向很高很散」根因）。
// 回归锚点 2（交互）：压缩模式为 BaseSettingsList 表单（源行尾按钮组 + 名称/格式/级别/
// 密码行 + 过滤开关），无待压缩列表、无进度条；密码经弹窗两次输入设置（zip/7z 均支持）。

const ARCHIVE_ENTRIES = [
  { path: 'readme.md', kind: 'file', size: 1024 },
  { path: 'src/main.rs', kind: 'file', size: 2048 },
  { path: 'src/lib.rs', kind: 'file', size: 4096 },
  { path: 'assets/', kind: 'dir', size: 0 },
  { path: 'assets/logo.png', kind: 'file', size: 102400 },
]

async function mockAndOpen(page: Page) {
  await page.addInitScript((entries) => {
    ;(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
      invoke: async (cmd: string, args: Record<string, unknown>) => {
        if (cmd === 'archive_inspect')
          return {
            path: '/tmp/demo/demo-project.zip',
            format: 'zip',
            entries,
            totalSize: 107568,
            encrypted: false,
            truncated: false,
          }
        if (cmd === 'archive_preview')
          return { payload: { type: 'text', content: '# Demo\nhello archive' }, truncated: false }
        if (cmd === 'pick_files') return []
        if (cmd === 'plugin:store|get')
          return args?.key === 'onboarded' ? [true, true] : [null, false]
        return null
      },
      metadata: {
        currentWindow: { label: 'main' },
        currentWebview: { windowLabel: 'main', label: 'main' },
      },
    }
  }, ARCHIVE_ENTRIES)
  await page.goto('/')
  await page.waitForSelector('#main-search-input', { timeout: 10000 })
  await page.evaluate(async () => {
    const el = document.querySelector('#app') as HTMLElement & {
      __vue_app__?: { config: { globalProperties: { $pinia?: Record<string, unknown> } } }
    }
    const pinia = el.__vue_app__?.config?.globalProperties?.$pinia
    if (!pinia) throw new Error('pinia not exposed')
    const hits = performance
      .getEntriesByType('resource')
      .map((e) => e.name)
      .filter((n) => n.includes('/src/stores/app.ts'))
    const { useAppStore } = await import(hits[hits.length - 1] ?? '/src/stores/app.ts')
    useAppStore(pinia).setActiveExtension('archive')
  })
}

function dispatch(page: Page, paths: string[]) {
  return page.evaluate((ps) => {
    window.dispatchEvent(new CustomEvent('archive-pending-input-path', { detail: ps }))
  }, paths)
}

test.describe('archive', () => {
  test.beforeEach(async ({ page }) => {
    await mockAndOpen(page)
  })

  test('解压模式：三行表单 + 二级文件树预览', async ({ page }) => {
    await dispatch(page, ['/tmp/demo/demo-project.zip'])
    await page.waitForTimeout(1200)

    // 表单三行：压缩包（尾随解压按钮）/ 解压到（默认同目录 + 选择）/ 查看压缩包
    const options = page.locator('.archive-view [role="option"]')
    await expect(options).toHaveCount(3, { timeout: 5000 })
    const archiveRow = page.locator('[role="option"]', { hasText: 'demo-project.zip' })
    await expect(archiveRow.getByText('ZIP · 5 个条目 · 105 KB')).toBeVisible()
    await expect(archiveRow.getByRole('button', { name: '解压', exact: true })).toBeVisible()
    await expect(page.getByRole('option', { name: /解压到/ })).toContainText('压缩包同目录')
    await expect(
      page.getByRole('option', { name: /查看压缩包/ }).locator('.fill-mist'),
    ).toBeVisible()

    // 回车进入二级文件树：目录展开（首层默认展开），6 个可见行（assets/logo.png + src/lib.rs、main.rs + readme.md）
    await page.keyboard.press('ArrowDown')
    await page.keyboard.press('ArrowDown')
    await page.keyboard.press('Enter')
    const treeOptions = page.locator('.archive-view [role="option"]')
    await expect(treeOptions).toHaveCount(6, { timeout: 5000 })
    await expect(page.getByRole('option', { name: /logo\.png/ })).toBeVisible()
    await expect(page.getByRole('option', { name: /src/ })).toBeVisible()

    // 文件行回车 = 预览（覆盖层出现；搜索框聚焦时空格归输入，回车是主通道）
    await expect(page.locator('.archive-preview-overlay')).toHaveCount(0)
    await page.keyboard.press('ArrowDown')
    await page.keyboard.press('Enter')
    await expect(page.locator('.archive-preview-overlay')).toBeVisible({ timeout: 5000 })
    await expect(page.locator('.archive-preview-overlay')).toContainText('# Demo')
    await page.keyboard.press('Escape')
    await expect(page.locator('.archive-preview-overlay')).toHaveCount(0)

    // 顶部搜索栏可输入过滤：输入 logo 后仅剩 logo.png 行
    await page.locator('#main-search-input').fill('logo')
    await expect(treeOptions).toHaveCount(1, { timeout: 5000 })

    // 按钮高度回归锚点：任何按钮 ≤ 40
    const btnHeights = await page.evaluate(() =>
      Array.from(document.querySelectorAll('.archive-view button')).map(
        (b) => b.getBoundingClientRect().height,
      ),
    )
    for (const h of btnHeights) expect(h).toBeLessThanOrEqual(40)
  })

  test('空态：先选操作方式，再选文件', async ({ page }) => {
    // 主界面搜索进入（无访达上下文）：默认解压 + 文件行（标题「文件」+ 右侧选择按钮）
    await expect(page.getByText('操作')).toBeVisible()
    await expect(page.getByText('解压', { exact: true })).toBeVisible()
    await expect(page.getByText('文件', { exact: true })).toBeVisible()
    await expect(page.getByRole('button', { name: '选择文件' })).toBeVisible()

    // 切「压缩」：按钮文案恒为「选择文件」，图标随操作切换
    await page.getByText('解压', { exact: true }).click()
    await page.getByText('压缩', { exact: true }).click()
    await expect(page.getByRole('button', { name: '选择文件' })).toBeVisible()

    // 文件行回车 = 触发选文件（picker mock 返回 null 无跳变，按钮仍在）
    await page.evaluate(() => (document.activeElement ?? {}).blur?.())
    await page.keyboard.press('ArrowDown')
    await page.keyboard.press('Enter')
    await expect(page.getByRole('button', { name: '选择文件' })).toBeVisible()
  })

  test('压缩模式：表单行结构 + 密码弹窗两次输入', async ({ page }) => {
    await dispatch(page, ['/tmp/demo/报告.pdf', '/tmp/demo/图/截图.png'])
    await page.waitForTimeout(1000)

    // 表单要素：源行（无图标、尾随按钮组）/ 名称输入 / 格式 / 级别预设 / 密码行 / 过滤开关
    await expect(page.getByText('压缩 2 项')).toBeVisible()
    await expect(page.getByText('归档名称')).toBeVisible()
    await expect(page.getByText('压缩级别')).toBeVisible()
    await expect(page.getByText('过滤系统垃圾文件')).toBeVisible()
    await expect(page.getByText('过滤隐藏文件')).toBeVisible()
    // 源行尾按钮组：仅主按钮（重新选择已移除）；源行无图标井
    const sourceRow = page.locator('[role="option"]', { hasText: '压缩 2 项' })
    await expect(sourceRow.getByText('压缩', { exact: true })).toBeVisible()
    await expect(page.getByText('重新选择')).toHaveCount(0)
    expect(await sourceRow.locator('.fill-mist').count()).toBe(0)
    // 仅名称一个输入框（密码行是按钮非输入框）
    expect(await page.locator('.archive-view input').count()).toBe(1)
    // 不展示待压缩项列表（源摘要行合法含首名预览；第二项不得作为行出现）
    await expect(page.getByRole('option', { name: /截图\.png/ })).toHaveCount(0)

    // 密码行回车 = 打开弹窗（键盘可达性，与行尾按钮同动作）；
    // 在任何 BaseSelect 交互前执行——select 收起后残留聚焦的控件会让列表键盘让位
    await page.keyboard.press('ArrowDown') // name
    await page.keyboard.press('ArrowDown') // format
    await page.keyboard.press('ArrowDown') // level
    await page.keyboard.press('ArrowDown') // password
    await page.keyboard.press('Enter')
    await expect(page.locator('[role="dialog"]')).toBeVisible()
    await page.keyboard.press('Escape')

    // 级别预设三档（点当前值展开 BaseSelect）
    await expect(page.getByText('标准')).toBeVisible()
    await page.getByText('标准').click()
    await expect(page.getByText('最快')).toBeVisible()
    await expect(page.getByText('最小')).toBeVisible()
    await page.keyboard.press('Escape')

    // 密码弹窗：两次输入；不一致报错，一致后行尾按钮变「已设置」
    await expect(page.getByRole('button', { name: '未加密' })).toBeVisible()
    await page.getByRole('button', { name: '未加密' }).click()
    await expect(page.locator('[role="dialog"]')).toBeVisible()
    await page.locator('[role="dialog"] input').first().fill('pw')
    await page.locator('[role="dialog"] input').nth(1).fill('pw2')
    await page.getByRole('dialog').getByRole('button', { name: '确定' }).click()
    await expect(page.getByText('两次输入不一致')).toBeVisible()
    await page.locator('[role="dialog"] input').nth(1).fill('pw')
    await page.getByRole('dialog').getByRole('button', { name: '确定' }).click()
    await expect(page.getByRole('button', { name: '已设置' })).toBeVisible()

    // 按钮高度回归锚点
    const btnH = await page.evaluate(() => {
      const b = document.querySelector('.archive-view button') as HTMLElement
      return b.getBoundingClientRect().height
    })
    expect(btnH).toBeLessThanOrEqual(40)
  })
})
