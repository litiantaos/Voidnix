import { test, expect, type Page } from '@playwright/test'

// 访达工具「用 App 打开」：候选平铺在面板（MRU 置顶 + LaunchServices 类型推荐 join，
// 无二级界面）。纯浏览器 mock __TAURI_INTERNALS__：finder_open_with_apps 模拟 LS 推荐，
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

async function enterFinderExt(page: Page, selected: string[]) {
  await page.addInitScript(
    ({ apps, lsApps, selected: sel }) => {
      ;(window as unknown as Record<string, unknown>).__finderAction = null
      let cbId = 0
      const eventCbs = new Map<number, (payload: unknown) => void>()
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
    },
    { apps: APPS, lsApps: LS_APPS, selected },
  )
  await page.goto('/')
  await page.waitForSelector('#main-search-input', { timeout: 10000 })
  const input = page.locator('#main-search-input')
  await input.fill('/finder')
  await page.waitForTimeout(200)
  await input.press('Enter')
  await expect(page.getByText('拷贝路径')).toBeVisible({ timeout: 5000 })
}

test.describe('finder-ext 用 App 打开', () => {
  test('候选组平铺：MRU 置顶 + LS 补足（join 不上的剔除），无二级入口', async ({ page }) => {
    await enterFinderExt(page, ['/Users/mock/Documents/project'])
    const headers = page.locator('.group-header')
    await expect(headers.filter({ hasText: '用 App 打开' })).toBeVisible({ timeout: 5000 })
    await expect(page.getByRole('option', { name: 'Mock Code' })).toBeVisible()
    // MRU（Mock Code）→ LS 偏好序（Safari、备忘录）；脏项 NotInstalled 不出现
    const group = page.locator('[role="option"]')
    const titles: string[] = []
    for (let i = 0; i < 3; i++) titles.push(await group.nth(i).innerText())
    expect(titles.join('|')).toBe('Mock Code|Safari|备忘录')
    await expect(page.getByRole('option', { name: 'NotInstalled' })).toHaveCount(0)
  })

  test('候选行应用图标缩小一半（57%）保留 fill-mist 外框', async ({ page }) => {
    await enterFinderExt(page, ['/Users/mock/Documents/project'])
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
    await enterFinderExt(page, ['/Users/mock/Documents/project'])
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
    await enterFinderExt(page, ['/Users/mock/Documents/project'])
    await expect(page.getByRole('option', { name: 'Mock Code' })).toBeVisible({ timeout: 5000 })
    await page.keyboard.press('Escape')
    await expect(page.locator('.ext-tag')).toHaveCount(0, { timeout: 5000 })
  })

  test('无选中时仅 MRU 候选（目标目录回退由 Rust 兜底）', async ({ page }) => {
    await enterFinderExt(page, [])
    await expect(page.getByRole('option', { name: 'Mock Code' })).toBeVisible({ timeout: 5000 })
    await expect(page.getByRole('option', { name: 'Safari' })).toHaveCount(0)
    await expect(page.getByRole('option', { name: '备忘录' })).toHaveCount(0)
  })
})
