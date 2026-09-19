import { test, expect, type Page } from '@playwright/test'

// 访达工具双进入模式：快捷键（shortcut-pressed 模拟）= 访达上下文面板（选区探测 +
// 「用 App 打开」候选平铺：MRU 置顶 + LaunchServices 类型推荐 join，无二级界面）；
// 应用界面进入（搜索 / 工具列表）= 浏览模式（全量操作目录，回车提示仅在访达中生效）。
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

/** 访达快捷键进入：上下文模式（探测选区 + 候选组） */
async function enterViaShortcut(page: Page, selected: string[]) {
  await setupFinderMock(page, selected)
  await page.evaluate(() => {
    ;(window as unknown as Record<string, unknown>).__pressShortcut()
  })
  await expect(page.getByRole('option', { name: 'Mock Code' })).toBeVisible({ timeout: 5000 })
}

test.describe('finder-ext 快捷键进入（访达上下文面板）', () => {
  test('候选组平铺：MRU 置顶 + LS 补足（join 不上的剔除），无二级入口', async ({ page }) => {
    await enterViaShortcut(page, ['/Users/mock/Documents/project'])
    const headers = page.locator('.group-header')
    await expect(headers.filter({ hasText: '用 App 打开' })).toBeVisible({ timeout: 5000 })
    await expect(page.getByRole('option', { name: 'Mock Code' })).toBeVisible()
    // MRU（Mock Code）→ LS 偏好序（Safari、备忘录）；脏项 NotInstalled 不出现；
    // 操作列表无 open_with 目录行（由候选组承载）
    const group = page.locator('[role="option"]')
    const titles: string[] = []
    for (let i = 0; i < 3; i++) titles.push(await group.nth(i).innerText())
    expect(titles.join('|')).toBe('Mock Code|Safari|备忘录')
    await expect(page.getByRole('option', { name: 'NotInstalled' })).toHaveCount(0)
    // 操作列表不重复出现 open_with 目录行（上下文模式由候选组承载）
    await expect(page.getByRole('option', { name: '用 App 打开' })).toHaveCount(0)
  })

  test('候选行应用图标缩小一半（57%）保留 fill-mist 外框', async ({ page }) => {
    await enterViaShortcut(page, ['/Users/mock/Documents/project'])
    await expect(page.getByRole('option', { name: 'Mock Code' })).toBeVisible({ timeout: 5000 })
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

  test('回车执行候选首行（MRU）：finder_run_action 携 appPath', async ({ page }) => {
    await enterViaShortcut(page, ['/Users/mock/Documents/project'])
    await expect(page.getByRole('option', { name: 'Mock Code' })).toBeVisible({ timeout: 5000 })
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

  test('Esc 直接退出扩展（无层级）', async ({ page }) => {
    await enterViaShortcut(page, ['/Users/mock/Documents/project'])
    await expect(page.getByRole('option', { name: 'Mock Code' })).toBeVisible({ timeout: 5000 })
    await page.keyboard.press('Escape')
    await expect(page.locator('.ext-tag')).toHaveCount(0, { timeout: 5000 })
  })

  test('无选中时仅 MRU 候选（目标目录回退由 Rust 兜底）', async ({ page }) => {
    await enterViaShortcut(page, [])
    await expect(page.getByRole('option', { name: 'Mock Code' })).toBeVisible({ timeout: 5000 })
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
      '拷贝路径',
      '在终端中打开',
      '新建文件',
      '切换隐藏文件',
      expect.stringContaining('启动快捷键'),
    ])
    // 无候选组（不探测选区）：无组头、无候选应用行
    await expect(page.locator('.group-header').filter({ hasText: '用 App 打开' })).toHaveCount(0)
    await expect(page.getByRole('option', { name: 'Mock Code' })).toHaveCount(0)
  })

  test('回车提示仅在访达中生效，不执行 finder_run_action', async ({ page }) => {
    await enterViaSearch(page, ['/Users/mock/Documents/project'])
    // 首行（用 App 打开）回车：仅提示，不执行命令
    await page.keyboard.press('Enter')
    await expect(page.getByText('该操作仅在访达中生效')).toBeVisible({ timeout: 5000 })
    const action = await page.evaluate(
      () => (window as unknown as Record<string, unknown>).__finderAction,
    )
    expect(action).toBeNull()
    // 媒体入口（视频处理）回车：同样仅提示，不跳转扩展
    await page.keyboard.press('ArrowDown')
    await page.keyboard.press('Enter')
    await expect(page.getByText('该操作仅在访达中生效').first()).toBeVisible({ timeout: 5000 })
    await expect(page.locator('.ext-tag')).toHaveText('访达工具')
  })

  test('浏览模式中按快捷键切换为访达上下文面板（候选组出现）', async ({ page }) => {
    await enterViaSearch(page, ['/Users/mock/Documents/project'])
    await page.evaluate(() => {
      ;(window as unknown as Record<string, unknown>).__pressShortcut()
    })
    await expect(page.locator('.group-header').filter({ hasText: '用 App 打开' })).toBeVisible({
      timeout: 5000,
    })
    await expect(page.getByRole('option', { name: 'Mock Code' })).toBeVisible()
    // 上下文模式回车执行候选（不再提示）
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
    await expect(page.locator('.group-header').filter({ hasText: '用 App 打开' })).toBeVisible({
      timeout: 5000,
    })
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
