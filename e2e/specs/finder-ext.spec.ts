import { test, expect, type Page } from '@playwright/test'

// 访达工具双进入模式：快捷键（shortcut-pressed 模拟）= 访达上下文面板（选区探测 +
// 「用 App 打开」目录行，回车进入二级界面显示具体 App 列表：MRU 置顶 +
// LaunchServices 类型推荐 join（仅相关应用，不补全已安装全量）；应用界面进入（搜索 / 工具列表）=
// 浏览模式（全量操作目录，回车提示仅在访达中生效）。
// 纯浏览器 mock __TAURI_INTERNALS__：finder_open_with_apps 模拟 LS 推荐，
// search_apps 模拟已安装列表（带 base64 图标走真实 img 渲染路径）。
// 1x1 PNG，验证 57% 缩放与透明底
const ICON_B64 =
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg=='

const APPS = [
  { id: 'app-1', title: 'Mock Code', path: '/Applications/Mock Code.app', kind: 'application' },
  { id: 'app-2', title: 'Safari', path: '/Applications/Safari.app', kind: 'application' },
  { id: 'app-3', title: '备忘录', path: '/System/Applications/Notes.app', kind: 'application' },
].map((a) => ({
  ...a,
  icon: ICON_B64,
  last_used: null,
  score: null,
  use_count: 0,
  parent: null,
}))

// LS 偏好序：默认应用在前；NotInstalled join 不上（脏项剔除）
const LS_APPS = [
  '/Applications/Safari.app',
  '/System/Applications/Notes.app',
  '/Applications/NotInstalled.app',
]

async function setupFinderMock(page: Page, selected: string[]) {
  await page.addInitScript(
    ({ apps, lsApps, selected: sel }) => {
      ;(window as unknown as Record<string, unknown>).__finderAction = null
      // toast 已迁移原生 NSPanel（无页面内 DOM），mock 捕获 show_toast 调用参数供断言
      ;(window as unknown as Record<string, { message: string; kind: string }[]>).__toastCalls = []
      let cbId = 0
      const eventCbs = new Map<number, (payload: unknown) => void>()
      const listenEvents = new Map<number, string>()
      ;(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
        transformCallback: (cb: (payload: unknown) => void) => {
          eventCbs.set(++cbId, cb)
          return cbId
        },
        invoke: async (cmd: string, args: Record<string, unknown>) => {
          switch (cmd) {
            case 'plugin:store|load':
              return 1
            case 'plugin:store|get':
              if (args?.key === 'onboarded') return [true, true]
              if (args?.key === 'recentApps') return [['/Applications/Mock Code.app'], true]
              return [null, false]
            case 'plugin:event|listen':
              listenEvents.set(args?.handler as number, String(args?.event))
              return null
            case 'search_apps':
              return apps
            case 'get_app_icons':
              return []
            case 'finder_selected_paths':
              return sel
            case 'finder_open_with_apps':
              return lsApps
            case 'finder_run_action':
              ;(window as unknown as Record<string, unknown>).__finderAction = {
                action: args?.action,
                name: args?.name ?? null,
                appPath: args?.appPath ?? null,
              }
              return ''
            case 'show_toast':
              ;(
                window as unknown as Record<string, { message: string; kind: string }[]>
              ).__toastCalls.push({
                message: String(args?.message),
                kind: String(args?.kind),
              })
              return null
            default:
              return null
          }
        },
        metadata: {
          currentWindow: { label: 'main' },
          currentWebview: { windowLabel: 'main', label: 'main' },
        },
      }
      // 模拟 Rust 侧全局快捷键（真实路径：访达中按 Option+F → shortcut-pressed →
      // makeToggleHandler 置 entryViaShortcut + tick，同步先于 View 挂载）
      ;(window as unknown as Record<string, unknown>).__pressShortcut = () => {
        for (const [handler, event] of listenEvents) {
          if (event === 'shortcut-pressed') {
            eventCbs.get(handler)?.({
              event,
              id: handler,
              payload: { id: 'finder-ext', wasVisible: false },
            })
          }
        }
      }
    },
    { apps: APPS, lsApps: LS_APPS, selected },
  )
  await page.goto('/')
  await page.waitForSelector('#main-search-input', { timeout: 10000 })
}

/** 应用界面进入（搜索 → 回车）：浏览模式 */
async function enterViaSearch(page: Page, selected: string[]) {
  await setupFinderMock(page, selected)
  const input = page.locator('#main-search-input')
  await input.fill('/finder')
  await page.waitForTimeout(200)
  await input.press('Enter')
  await expect(page.getByText('切换隐藏文件')).toBeVisible({ timeout: 5000 })
}

/** 访达快捷键进入：上下文模式（探测选区，面板显示操作目录） */
async function enterViaShortcut(page: Page, selected: string[]) {
  await setupFinderMock(page, selected)
  await page.evaluate(() => {
    ;(window as unknown as Record<string, unknown>).__pressShortcut()
  })
  await expect(page.getByRole('option', { name: '用 App 打开' })).toBeVisible({ timeout: 5000 })
}

/** 面板回车进入「用 App 打开」二级界面（App 列表） */
async function enterOpenWith(page: Page) {
  await page.keyboard.press('Enter')
  await expect(page.getByRole('option', { name: 'Mock Code' })).toBeVisible({ timeout: 5000 })
}

test.describe('finder-ext 快捷键进入（访达上下文面板）', () => {
  test('open_with 目录行承载（无候选平铺），副标题显示选区文件名', async ({ page }) => {
    await enterViaShortcut(page, ['/Users/mock/Documents/project'])
    // 面板无候选组、无 App 行；open_with 目录行带选区副标题置顶
    await expect(page.locator('.group-header').filter({ hasText: '用 App 打开' })).toHaveCount(0)
    await expect(page.getByRole('option', { name: 'Mock Code' })).toHaveCount(0)
    const first = page.locator('[role="option"]').first()
    await expect(first).toContainText('用 App 打开')
    await expect(first).toContainText('project')
  })

  test('回车进入二级界面：MRU 置顶 + LS 类型推荐（join 不上的剔除，不补全全量）', async ({
    page,
  }) => {
    await enterViaShortcut(page, ['/Users/mock/Documents/project'])
    await enterOpenWith(page)
    // MRU（Mock Code）→ LS 偏好序（Safari、备忘录）；脏项 NotInstalled join 不上剔除
    const rows = page.locator('[role="option"]')
    const titles: string[] = []
    for (let i = 0; i < 3; i++) titles.push(await rows.nth(i).innerText())
    expect(titles.join('|')).toBe('Mock Code|Safari|备忘录')
    await expect(rows).toHaveCount(3)
    await expect(page.getByRole('option', { name: 'NotInstalled' })).toHaveCount(0)
  })

  test('二级界面搜索框过滤（输入即筛）', async ({ page }) => {
    await enterViaShortcut(page, ['/Users/mock/Documents/project'])
    await enterOpenWith(page)
    const input = page.locator('#main-search-input')
    await input.fill('Safari')
    await expect(page.getByRole('option', { name: 'Mock Code' })).toHaveCount(0, {
      timeout: 5000,
    })
    await expect(page.getByRole('option', { name: 'Safari' })).toBeVisible()
    // 清空恢复完整相关列表
    await input.fill('')
    await expect(page.getByRole('option', { name: 'Mock Code' })).toBeVisible({ timeout: 5000 })
  })

  test('二级界面应用图标缩小一半（57%）保留 fill-mist 外框', async ({ page }) => {
    await enterViaShortcut(page, ['/Users/mock/Documents/project'])
    await enterOpenWith(page)
    const size = await page.evaluate(() => {
      const img = document.querySelector('[role="option"] img')
      if (!img) return null
      const wrapper = img.parentElement as HTMLElement
      return {
        ratio: img.clientWidth / wrapper.clientWidth,
        wrapperBg: getComputedStyle(wrapper).backgroundColor,
      }
    })
    expect(size).not.toBeNull()
    expect(size!.ratio).toBeGreaterThan(0.5)
    expect(size!.ratio).toBeLessThan(0.62)
    // 外框保留：fill-mist 底非透明
    expect(size!.wrapperBg).not.toBe('rgba(0, 0, 0, 0)')
  })

  test('二级界面回车执行首行（MRU）：finder_run_action 携 appPath', async ({ page }) => {
    await enterViaShortcut(page, ['/Users/mock/Documents/project'])
    await enterOpenWith(page)
    await page.keyboard.press('Enter')
    const action = await page.evaluate(
      () => (window as unknown as Record<string, unknown>).__finderAction,
    )
    expect(action).toEqual({
      action: 'open_with',
      name: null,
      appPath: '/Applications/Mock Code.app',
    })
  })

  test('Esc 逐层退出：二级界面 → 面板 → 退出扩展', async ({ page }) => {
    await enterViaShortcut(page, ['/Users/mock/Documents/project'])
    await enterOpenWith(page)
    await page.keyboard.press('Escape')
    // 回到面板：目录行重现、App 行消失，扩展仍在
    await expect(page.getByRole('option', { name: '用 App 打开' })).toBeVisible({
      timeout: 5000,
    })
    await expect(page.getByRole('option', { name: 'Mock Code' })).toHaveCount(0)
    await expect(page.locator('.ext-tag')).toHaveText('访达工具')
    await page.keyboard.press('Escape')
    await expect(page.locator('.ext-tag')).toHaveCount(0, { timeout: 5000 })
  })

  test('二级界面返回面板保留选区上下文（不重置为浏览模式）', async ({ page }) => {
    await enterViaShortcut(page, ['/Users/mock/Documents/project.mp4'])
    // 选区为视频：媒体入口出现 + open_with 副标题带文件名
    await expect(page.getByRole('option', { name: '视频处理' })).toBeVisible({ timeout: 5000 })
    await enterOpenWith(page)
    await page.keyboard.press('Escape')
    // 返回面板：仍为访达上下文（视频入口与副标题保留，不翻转浏览模式清空选区）
    await expect(page.getByRole('option', { name: '视频处理' })).toBeVisible({ timeout: 5000 })
    const first = page.locator('[role="option"]').first()
    await expect(first).toContainText('用 App 打开')
    await expect(first).toContainText('project.mp4')
  })

  test('跨扩展跳转（视频处理）Esc 返回保留选区上下文', async ({ page }) => {
    await enterViaShortcut(page, ['/Users/mock/video.mp4'])
    await expect(page.getByRole('option', { name: '视频处理' })).toBeVisible({ timeout: 5000 })
    // 选中「视频处理」行（open_with 后第二行）回车跳转 video 扩展
    await page.keyboard.press('ArrowDown')
    await page.keyboard.press('Enter')
    await expect(page.locator('.ext-tag')).toHaveText('视频处理', { timeout: 5000 })
    // Esc 经返回目标回到访达面板：仍为上下文模式（视频入口与副标题保留）
    await page.keyboard.press('Escape')
    await expect(page.getByRole('option', { name: '用 App 打开' })).toBeVisible({ timeout: 5000 })
    await expect(page.getByRole('option', { name: '视频处理' })).toBeVisible()
    const first = page.locator('[role="option"]').first()
    await expect(first).toContainText('video.mp4')
  })

  test('二级界面期间快捷键重呼不泄漏进入标记（退出再进仍为浏览模式）', async ({ page }) => {
    await enterViaShortcut(page, ['/Users/mock/Documents/project'])
    await enterOpenWith(page)
    // subview 让位停用态下快捷键重呼（窗口隐藏后再呼出）：makeToggleHandler 的
    // setActiveExtension 清 subview 回面板根（contextResumable 消费保持上下文），且
    // tick watch 无条件消费进入标记
    await page.evaluate(() => {
      ;(window as unknown as Record<string, unknown>).__pressShortcut()
    })
    await expect(page.getByRole('option', { name: '用 App 打开' })).toBeVisible({ timeout: 5000 })
    await expect(page.locator('[role="option"]').first()).toContainText('project')
    await expect(page.getByRole('option', { name: 'Mock Code' })).toHaveCount(0)
    // Esc 退出扩展，经搜索再进入：进入标记已被消费（无残留），应为浏览模式
    await page.keyboard.press('Escape')
    await expect(page.locator('.ext-tag')).toHaveCount(0, { timeout: 5000 })
    const input = page.locator('#main-search-input')
    await input.fill('/finder')
    await page.waitForTimeout(200)
    await input.press('Enter')
    await expect(page.getByRole('option', { name: '用 App 打开' })).toBeVisible({ timeout: 5000 })
    // 浏览模式：无选区副标题、无 App 行
    await expect(page.locator('[role="option"]').first()).not.toContainText('project')
    await expect(page.getByRole('option', { name: 'Mock Code' })).toHaveCount(0)
  })

  test('无选中时二级界面仅 MRU（目标目录回退由 Rust 兜底）', async ({ page }) => {
    await enterViaShortcut(page, [])
    await enterOpenWith(page)
    // LS 空（无选区）：仅 MRU（Mock Code），不补全已安装全量
    await expect(page.getByRole('option', { name: 'Mock Code' })).toBeVisible()
    await expect(page.getByRole('option', { name: 'Safari' })).toHaveCount(0)
    await expect(page.getByRole('option', { name: '备忘录' })).toHaveCount(0)
  })
})

test.describe('finder-ext 应用界面进入（浏览模式）', () => {
  test('显示全量操作目录（含媒体入口），序与上下文面板一致', async ({ page }) => {
    await enterViaSearch(page, ['/Users/mock/Documents/project'])
    // 全量目录（含 open_with 目录行 + 视频/图片处理目录行），顺序与访达快捷键面板一致；
    // 媒体入口无选区副标题（目录行形态）
    const rows = page.locator('[role="option"]')
    const count = await rows.count()
    const titles: string[] = []
    for (let i = 0; i < count; i++) titles.push(await rows.nth(i).innerText())
    expect(titles).toEqual([
      '用 App 打开',
      '视频处理',
      '图片处理',
      '压缩 / 解压',
      '拷贝路径',
      '在终端中打开',
      '新建文件',
      '切换隐藏文件',
      expect.stringContaining('呼出快捷键'),
    ])
    // 无候选组（不探测选区）：无组头、无候选应用行
    await expect(page.locator('.group-header').filter({ hasText: '用 App 打开' })).toHaveCount(0)
    await expect(page.getByRole('option', { name: 'Mock Code' })).toHaveCount(0)
  })

  test('回车提示仅在访达中生效，不执行 finder_run_action', async ({ page }) => {
    await enterViaSearch(page, ['/Users/mock/Documents/project'])
    // 首行（用 App 打开）回车：仅提示，不执行命令（原生 toast，断言 mock 捕获的调用）
    await page.keyboard.press('Enter')
    const toastCalls = () =>
      page.evaluate(
        () =>
          (window as unknown as Record<string, { message: string; kind: string }[]>).__toastCalls,
      )
    expect(await toastCalls()).toEqual([{ message: '请先切换到访达', kind: 'success' }])
    const action = await page.evaluate(
      () => (window as unknown as Record<string, unknown>).__finderAction,
    )
    expect(action).toBeNull()
    // 媒体入口（视频处理）回车：同样仅提示，不跳转扩展
    await page.keyboard.press('ArrowDown')
    await page.keyboard.press('Enter')
    expect((await toastCalls()).at(-1)?.message).toBe('请先切换到访达')
    await expect(page.locator('.ext-tag')).toHaveText('访达工具')
  })

  test('浏览模式中按快捷键切换为访达上下文面板（open_with 可进入二级界面）', async ({ page }) => {
    await enterViaSearch(page, ['/Users/mock/Documents/project'])
    await page.evaluate(() => {
      ;(window as unknown as Record<string, unknown>).__pressShortcut()
    })
    // 上下文模式：open_with 副标题带选区、回车进入二级界面执行（不再提示）
    const first = page.locator('[role="option"]').first()
    await expect(first).toContainText('用 App 打开', { timeout: 5000 })
    await expect(first).toContainText('project')
    await enterOpenWith(page)
    await page.keyboard.press('Enter')
    const action = await page.evaluate(
      () => (window as unknown as Record<string, unknown>).__finderAction,
    )
    expect(action).toEqual({
      action: 'open_with',
      name: null,
      appPath: '/Applications/Mock Code.app',
    })
  })

  test('快捷键重入不残留进入标记，应用界面再进入仍为浏览模式', async ({ page }) => {
    await enterViaSearch(page, ['/Users/mock/Documents/project'])
    // 视图激活态下快捷键重入（模拟窗口隐藏后再呼出，wasVisible=false，无 onActivated 跟进）：
    // 切为上下文面板
    await page.evaluate(() => {
      ;(window as unknown as Record<string, unknown>).__pressShortcut()
    })
    await expect(page.getByRole('option', { name: '用 App 打开' })).toBeVisible({ timeout: 5000 })
    await expect(page.getByRole('option', { name: 'Mock Code' })).toHaveCount(0)
    // 退出扩展，经搜索再进入：进入标记已被消费（无残留），应为浏览模式而非上下文面板
    await page.keyboard.press('Escape')
    await expect(page.locator('.ext-tag')).toHaveCount(0, { timeout: 5000 })
    const input = page.locator('#main-search-input')
    await input.fill('/finder')
    await page.waitForTimeout(200)
    await input.press('Enter')
    await expect(page.getByRole('option', { name: '用 App 打开' })).toBeVisible({ timeout: 5000 })
    await expect(page.locator('.group-header').filter({ hasText: '用 App 打开' })).toHaveCount(0)
  })
})
