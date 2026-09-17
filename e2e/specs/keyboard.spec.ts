import { test, expect, type Page } from '@playwright/test'

// 后台停用扩展的列表不得劫持键盘：停用视图的响应式 watcher 仍活跃，其内部 v-if
// 翻转会使列表在停用 KeepAlive 子树内卸载后重挂载（剪贴板 history 随 query 过滤
// 清空再回填）——重挂载不触发 activated/deactivated 钩子对，修复前列表在后台仍
// 消费 ↑↓ 与 Enter，用户在其它扩展内回车会误触剪贴板粘贴。
// 纯浏览器 mock __TAURI_INTERNALS__（事件回调可触发，模拟 Rust 侧全局快捷键事件
// shortcut-pressed——真实路径不经过 query，保证僵尸列表监听先于目标视图注册）
test.describe('列表键盘停用隔离', () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(() => {
      const items = Array.from({ length: 10 }, (_, i) => ({
        id: String(100 + i),
        content: `mock item ${i + 1}`,
        content_type: 'text',
        source_app: 'Mock',
        created_at: '2026-09-17 10:00:00',
        is_favorite: false,
        score: 0,
        file_size: null,
        image_width: null,
        image_height: null,
      }))
      let cbId = 0
      const eventCbs = new Map<number, (payload: unknown) => void>()
      const listenEvents = new Map<number, string>()
      ;(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
        transformCallback: (cb: (payload: unknown) => void) => {
          eventCbs.set(++cbId, cb)
          return cbId
        },
        invoke: async (cmd: string, args: Record<string, unknown>) => {
          if (cmd === 'get_clipboard_history') {
            if (args?.limit === 1) return []
            return items
          }
          if (cmd === 'paste_clipboard_item') {
            ;(window as unknown as Record<string, unknown>).__zombiePaste = String(args?.id)
            return null
          }
          if (cmd === 'plugin:event|listen') {
            listenEvents.set(args?.handler as number, String(args?.event))
            return null
          }
          if (cmd === 'plugin:store|get')
            return args?.key === 'onboarded' ? [true, true] : [null, false]
          return null
        },
        metadata: {
          currentWindow: { label: 'main' },
          currentWebview: { windowLabel: 'main', label: 'main' },
        },
      }
      // 模拟 Rust 侧全局快捷键（真实路径：全局快捷键 → shortcut-pressed → 激活扩展，
      // 不经过 query 变化——后台重挂载列表的 document 监听得以先于目标视图注册）
      ;(window as unknown as Record<string, unknown>).__pressShortcut = (id: string) => {
        for (const [handler, event] of listenEvents) {
          if (event === 'shortcut-pressed') {
            eventCbs.get(handler)?.({ event, id: handler, payload: { id, wasVisible: true } })
          }
        }
      }
    })
    await page.goto('/')
    await page.waitForSelector('#main-search-input', { timeout: 10000 })
  })

  async function enterClipboard(page: Page) {
    const input = page.locator('#main-search-input')
    await input.fill('/')
    await page.waitForTimeout(200)
    await page
      .getByRole('option', { name: /剪贴板/ })
      .first()
      .click()
    await page.keyboard.press('Enter')
    await page.waitForTimeout(400)
    const rows = page.locator('[role="option"]')
    await expect(rows.first()).toBeVisible({ timeout: 5000 })
    return rows
  }

  test('后台重挂载的剪贴板列表不劫持其它扩展的导航与回车', async ({ page }) => {
    const input = page.locator('#main-search-input')

    // 进入剪贴板建立视图缓存，方向键导航正常（激活态列表自身不受影响）
    await enterClipboard(page)
    await page.keyboard.press('ArrowDown')
    await expect(page.locator('.ui-active')).toHaveText(/^mock item 2(?!\d)/)

    // Esc 回主界面：剪贴板视图停用（KeepAlive 缓存），其 searchQuery watcher 仍活跃
    await page.keyboard.press('Escape')
    await page.waitForTimeout(400)
    await expect(page.locator('.ext-tag')).toHaveCount(0, { timeout: 5000 })

    // 输入不命中任何剪贴板记录的 query：后台 history 被过滤清空 → 列表在停用树内卸载
    await input.fill('zzz')
    await page.waitForTimeout(300)

    // 清空 query：后台 history 回填 → 列表在停用树内重挂载（修复前成为响应键盘的僵尸）
    await input.fill('')
    await page.waitForTimeout(300)

    // 全局快捷键直开 agent（无 query 变化）：agent 视图监听注册在后台列表之后
    await page.evaluate((id) => {
      ;(window as unknown as Record<string, unknown>).__pressShortcut(id)
    }, 'agent')
    await page.waitForTimeout(400)

    // agent 内方向键 + 回车：不得触发剪贴板粘贴（修复前回车被后台列表消费，粘贴其选中项）
    await page.keyboard.press('ArrowDown')
    await page.keyboard.press('Enter')
    await page.waitForTimeout(300)
    const pasted = await page.evaluate(
      () => (window as unknown as Record<string, unknown>).__zombiePaste,
    )
    expect(pasted).toBeUndefined()

    // 正向对照：快捷键回到剪贴板（重新激活），导航与回车恢复正常——
    // 选中经跨会话转移归首项，↓ 选中第二项，回车正常粘贴该记录（不过度抑制）
    await page.evaluate((id) => {
      ;(window as unknown as Record<string, unknown>).__pressShortcut(id)
    }, 'clipboard')
    await page.waitForTimeout(400)
    await expect(page.locator('.ui-active')).toHaveText(/^mock item 1(?!\d)/)
    await page.keyboard.press('ArrowDown')
    await expect(page.locator('.ui-active')).toHaveText(/^mock item 2(?!\d)/)
    await page.keyboard.press('Enter')
    await page.waitForTimeout(300)
    const pastedActive = await page.evaluate(
      () => (window as unknown as Record<string, unknown>).__zombiePaste,
    )
    expect(pastedActive).toBe('101')
  })

  test('多选后 ESC 先清选择不退出扩展（捕获相先行，不受挂载顺序影响）', async ({ page }) => {
    await enterClipboard(page)

    // shift+↓ 范围选择两项（第 2、3 项均高亮）
    await page.keyboard.press('ArrowDown')
    await page.keyboard.down('Shift')
    await page.keyboard.press('ArrowDown')
    await page.keyboard.up('Shift')
    await page.waitForTimeout(200)
    await expect(page.locator('.ui-active')).toHaveCount(2)

    // 第一次 Esc：仅清多选——仍在剪贴板（不退出），高亮回到单选一项
    await page.keyboard.press('Escape')
    await page.waitForTimeout(300)
    await expect(page.locator('.ext-tag')).toHaveCount(1)
    await expect(page.locator('.ui-active')).toHaveCount(1)

    // 第二次 Esc：无多选，正常退出扩展
    await page.keyboard.press('Escape')
    await page.waitForTimeout(400)
    await expect(page.locator('.ext-tag')).toHaveCount(0)
  })
})
