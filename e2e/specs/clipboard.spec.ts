import { test, expect, type Page } from '@playwright/test'

// 剪贴板扩展列表渲染依赖 get_clipboard_history；纯浏览器 mock __TAURI_INTERNALS__
// （isTauri=true + 该命令返回 40 条记录，onboarded=true 跳过首启引导，其余命令兜底 null）
test.describe('剪贴板进入重置', () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(() => {
      const items = Array.from({ length: 40 }, (_, i) => {
        // 尾段混排图片记录：按上键 wrap 到末项时上方有多个图片项，加载完成后高度跳变
        const isImage = i >= 31 && i % 2 === 0
        return {
          id: String(100 + i),
          content: isImage ? `/mock/shot-${i + 1}.png` : `mock item ${i + 1}`,
          content_type: isImage ? 'image' : 'text',
          source_app: 'Mock',
          created_at: '2026-09-13 10:00:00',
          is_favorite: false,
          score: 0,
          file_size: null,
          image_width: null,
          image_height: null,
        }
      })
      // 1×1 透明 PNG
      const pngData =
        'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg=='
      // listen() 注册回调前同步取事件 id（缺省会使 useAppLifecycle 等生命周期监听注册中断）
      let cbId = 0
      ;(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
        transformCallback: () => ++cbId,
        invoke: async (cmd: string, args: Record<string, unknown>) => {
          if (cmd === 'get_clipboard_history') {
            // limit:1 是唤起填充探测（maybeFillFromClipboard）：返回 3 秒内新文本，
            // 验证扩展激活时该特性不污染 query
            if (args?.limit === 1) {
              const now = new Date().toISOString().slice(0, 19).replace('T', ' ')
              return [{ ...items[39], content: 'freshly copied', created_at: now }]
            }
            // 每次返回新数组：忠实模拟 Rust serde 反序列化（引用必变），
            // 返回闭包原引用会使「原地 splice 删除 + 缓存命中」路径失去引用变化信号
            return items.slice()
          }
          // 模拟缩略图加载滞后（异步 IPC + 解码）；id=134（/mock/shot-35.png）模拟
          // 图片文件被外部清理：Rust 查不到文件返回 null，断言回落文本分支
          if (cmd === 'get_clipboard_image') {
            if (args?.id === '134') return null
            await new Promise((r) => setTimeout(r, 600))
            return pngData
          }
          // 删除命令原地裁剪闭包 items，后续 get_clipboard_history 返回裁剪后列表
          if (cmd === 'delete_clipboard_items') {
            const ids = new Set<string>(args?.ids ?? [])
            for (let i = items.length - 1; i >= 0; i--) {
              if (ids.has(items[i]!.id)) items.splice(i, 1)
            }
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

  async function scrollTop(page: Page) {
    return page.evaluate(
      () => document.querySelector<HTMLElement>('.hide-scrollbar')?.scrollTop ?? -1,
    )
  }

  test('退出扩展再进入：滚动与选中均重置（KeepAlive 往返不残留旧选中）', async ({ page }) => {
    const rows = await enterClipboard(page)
    expect(await rows.count()).toBe(40)

    // 方向键下移 15 格：选中离开视野，BaseList reveal 联动产生真实滚动
    for (let i = 0; i < 15; i++) await page.keyboard.press('ArrowDown')
    await page.waitForTimeout(200)
    expect(await scrollTop(page)).toBeGreaterThan(0)
    await expect(page.locator('.ui-active')).toContainText('mock item 16')

    // Esc 退出回主界面
    await page.keyboard.press('Escape')
    await page.waitForTimeout(400)
    await expect(page.locator('.ext-tag')).toHaveCount(0, { timeout: 5000 })

    // 再次进入：滚动归顶 + 选中归首项
    const rows2 = await enterClipboard(page)
    expect(await rows2.count()).toBe(40)
    expect(await scrollTop(page)).toBe(0)
    // 正则锚定完整行首（'mock item 1' 是 'mock item 16' 的子串，contains 会假绿）
    await expect(page.locator('.ui-active')).toHaveText(/^mock item 1(?!\d)/)
  })

  test('鼠标点击选中后经标签关闭退出再进入：滚动与选中均重置', async ({ page }) => {
    await enterClipboard(page)

    // 鼠标点击中间项（onItemClick → setSelectedIndex，与方向键同链路）
    await page.locator('[role="option"]').nth(20).click()
    await expect(page.locator('.ui-active')).toHaveText(/^mock item 21(?!\d)/)

    // hover 扩展标签显现 X 后点击退出（handleTagClose → exitExtension）
    await page.locator('.ext-tag').hover()
    await page.locator('.ext-tag-close').click()
    await page.waitForTimeout(400)
    await expect(page.locator('.ext-tag')).toHaveCount(0, { timeout: 5000 })

    const rows2 = await enterClipboard(page)
    expect(await rows2.count()).toBe(40)
    expect(await scrollTop(page)).toBe(0)
    await expect(page.locator('.ui-active')).toHaveText(/^mock item 1(?!\d)/)
  })

  test('subview 往返保留：主列表选中与滚动经 config 设置页往返后不重置', async ({ page }) => {
    await enterClipboard(page)

    for (let i = 0; i < 15; i++) await page.keyboard.press('ArrowDown')
    await page.waitForTimeout(200)
    const topBefore = await scrollTop(page)
    expect(topBefore).toBeGreaterThan(0)
    await expect(page.locator('.ui-active')).toHaveText(/^mock item 16(?!\d)/)

    // 进 config 设置页（accessory 设置按钮 toggle subview；按图标定位）
    const configBtn = page.locator('[data-search-bar-accessory] button:has(.i-ri-settings-3-line)')
    await configBtn.click()
    await page.waitForTimeout(300)
    // 主列表让位给设置页（设置项渲染 + 自身高亮归首项，滚动归顶）
    await expect(page.locator('[role="option"]').first()).toBeVisible({ timeout: 5000 })
    expect(await scrollTop(page)).toBe(0)
    await expect(page.locator('.ui-active')).not.toContainText('mock item')

    // 返回主视图：同扩展内往返（activeExtId 不变），滚动恢复、选中保留
    await page.locator('[data-search-bar-accessory] button:has(.i-ri-settings-3-fill)').click()
    await page.waitForTimeout(300)
    expect(await scrollTop(page)).toBe(topBefore)
    await expect(page.locator('.ui-active')).toHaveText(/^mock item 16(?!\d)/)
  })

  test('窗口隐藏即归位：选中归首项并滚顶（会话结束，下次唤起首帧即首项）', async ({ page }) => {
    const rows = await enterClipboard(page)
    expect(await rows.count()).toBe(40)

    for (let i = 0; i < 15; i++) await page.keyboard.press('ArrowDown')
    await page.waitForTimeout(200)
    const topHide = await scrollTop(page)
    expect(topHide).toBeGreaterThan(0)
    await expect(page.locator('.ui-active')).toHaveText(/^mock item 16(?!\d)/)

    // 模拟窗口隐藏（Tauri 下 hideWindow 派发；浏览器模式手动派发同一事件）。
    // hide 不 orderOut 架构下 show 立即可见的是隐藏前最后一帧——归位锚在隐藏
    // 时刻（DOM 更新在隐藏期完成），而非唤起时刻（旧选中位置会可见几十 ms）
    await page.evaluate(() => window.dispatchEvent(new CustomEvent('window-hiding')))
    await page.waitForTimeout(300)

    expect(await scrollTop(page)).toBe(0)
    await expect(page.locator('.ui-active')).toHaveText(/^mock item 1(?!\d)/)
  })

  test('过滤清空列表后退出再进入：跨会话归零不丢失（BaseList 常挂不卸载）', async ({ page }) => {
    await enterClipboard(page)

    for (let i = 0; i < 15; i++) await page.keyboard.press('ArrowDown')
    await page.waitForTimeout(200)
    await expect(page.locator('.ui-active')).toHaveText(/^mock item 16(?!\d)/)

    // 输入无匹配过滤词：history 清空（BaseList 常挂仅隐藏，行数为零）
    await page.locator('#main-search-input').fill('zzz-no-match')
    await page.waitForTimeout(400)
    await expect(page.locator('[role="option"]')).toHaveCount(0)

    // 此时退出扩展：BaseList 未卸载（v-show），其跨会话归零 watch 恒在
    await page.keyboard.press('Escape')
    await page.waitForTimeout(400)
    await expect(page.locator('.ext-tag')).toHaveCount(0, { timeout: 5000 })

    // 重新进入（history 回填使 BaseList 重挂载）：选中从首项起而非残留旧索引
    const rows2 = await enterClipboard(page)
    expect(await rows2.count()).toBe(40)
    expect(await scrollTop(page)).toBe(0)
    await expect(page.locator('.ui-active')).toHaveText(/^mock item 1(?!\d)/)
  })

  test('同会话过滤往返：过滤词变化与清空均归首项（内容全变不保留漂移索引）', async ({ page }) => {
    await enterClipboard(page)

    for (let i = 0; i < 15; i++) await page.keyboard.press('ArrowDown')
    await page.waitForTimeout(200)
    await expect(page.locator('.ui-active')).toHaveText(/^mock item 16(?!\d)/)

    // 输入过滤词：列表内容全变，选中归首项（不再保留指向漂移记录的旧索引）
    await page.locator('#main-search-input').fill('item 19')
    await page.waitForTimeout(400)
    const filtered = page.locator('[role="option"]')
    await expect(filtered).toHaveCount(1, { timeout: 5000 })
    await expect(page.locator('.ui-active')).toHaveText(/^mock item 19/)

    // 清空过滤词：全列表回填，选中归首项并滚顶
    await page.locator('#main-search-input').fill('')
    await page.waitForTimeout(400)
    await expect(page.locator('[role="option"]')).toHaveCount(40, { timeout: 5000 })
    expect(await scrollTop(page)).toBe(0)
    await expect(page.locator('.ui-active')).toHaveText(/^mock item 1(?!\d)/)
  })

  test('删除选中末项后列表缩短：选中贴尾不越界（连续删除无高亮丢失）', async ({ page }) => {
    await enterClipboard(page)

    // wrap 到末项（mock item 40）
    await page.keyboard.press('ArrowUp')
    await page.waitForTimeout(250)
    await expect(page.locator('.ui-active')).toHaveText(/^mock item 40/)

    // Cmd+Enter 开动作菜单 → 删除 → confirm 确认
    await page.keyboard.press('Meta+Enter')
    const menu = page.locator('[role="menu"]')
    await expect(menu).toBeVisible()
    await menu.getByText('删除').click()
    const dialog = page.getByRole('dialog')
    await expect(dialog).toBeVisible()
    await dialog.getByRole('button', { name: '删除' }).click()

    // 删除后 fetch 回填 39 条：选中越界（39 ≥ 39）clamp 贴尾至第 38 行
    // （mock item 39 为图片行，标题渲染为 img 不进 innerText，按行索引断言）
    await page.waitForTimeout(500)
    await expect(page.locator('[role="option"]')).toHaveCount(39, { timeout: 5000 })
    await expect(page.locator('[role="option"]').nth(38)).toHaveClass(/ui-active/)
  })

  test('回车粘贴后选中归首项（粘贴即会话结束，下次唤起首帧即首项）', async ({ page }) => {
    await enterClipboard(page)

    for (let i = 0; i < 15; i++) await page.keyboard.press('ArrowDown')
    await page.waitForTimeout(200)
    await expect(page.locator('.ui-active')).toHaveText(/^mock item 16(?!\d)/)

    // 回车粘贴：粘贴命令在 Rust 端隐藏窗口（前端 invoke 返回时已隐藏），
    // 成功分支立即归位——DOM 更新在隐藏期完成，下次唤起首帧即首项
    await page.keyboard.press('Enter')
    await page.waitForTimeout(300)
    await expect(page.locator('.ui-active')).toHaveText(/^mock item 1(?!\d)/)
    expect(await scrollTop(page)).toBe(0)
  })

  test('扩展激活时唤起填充特性不污染 query（窗口显隐不改扩展内容状态）', async ({ page }) => {
    const rows = await enterClipboard(page)
    expect(await rows.count()).toBe(40)

    // 模拟主快捷键从隐藏唤起（mock 的 limit:1 探测返回 3 秒内新文本「freshly copied」）
    await page.evaluate(() => window.dispatchEvent(new CustomEvent('window-invoked')))
    await page.waitForTimeout(300)

    // 扩展激活：填充特性跳过——query 保持空、列表不过滤、选中不动
    await expect(page.locator('#main-search-input')).toHaveValue('')
    expect(await rows.count()).toBe(40)
    await expect(page.locator('.ui-active')).toHaveText(/^mock item 1(?!\d)/)
  })

  test('工具列表浏览位置经扩展往返恢复（savedToolIndex 不被列表归零回写覆盖）', async ({
    page,
  }) => {
    const input = page.locator('#main-search-input')
    await input.fill('/')
    await page.waitForTimeout(200)
    const rows = page.locator('[role="option"]')
    await expect(rows.first()).toBeVisible({ timeout: 5000 })

    // 浏览到「时间戳」（第 6 项）后回车进入该扩展
    for (let i = 0; i < 5; i++) await page.keyboard.press('ArrowDown')
    await expect(page.locator('.ui-active')).toContainText('时间戳')
    await page.keyboard.press('Enter')
    await page.waitForTimeout(400)
    await expect(page.locator('.ext-tag')).toContainText('时间戳')

    // Esc 退出回工具列表：浏览位置恢复（选中仍在「时间戳」而非归零到首项）
    await page.keyboard.press('Escape')
    await page.waitForTimeout(400)
    await expect(page.locator('.ext-tag')).toHaveCount(0, { timeout: 5000 })
    await expect(rows.first()).toBeVisible({ timeout: 5000 })
    await expect(page.locator('.ui-active')).toContainText('时间戳')
  })

  test('按上键 wrap 到末项：图片缩略图加载完成后选中项保持在视口内（占位恒高）', async ({
    page,
  }) => {
    await enterClipboard(page)

    // 首次进入直接按上键：wrap 选中最后一项（text 项），reveal 滚入视野底部
    await page.keyboard.press('ArrowUp')
    await page.waitForTimeout(250)
    await expect(page.locator('.ui-active')).toHaveText(/^mock item 40/)

    // 占位恒高：图片加载前后列表总高度不变（占位与缩略图精确等高）
    const heightBefore = await page.evaluate(
      () => document.querySelector('.hide-scrollbar')?.scrollHeight ?? -1,
    )
    // 上方图片项（32/34/36/38）在 200px 预载范围内陆续加载完成，高度跳变后
    // 选中项不得被推出视口
    await page.waitForTimeout(1100)
    const heightAfter = await page.evaluate(
      () => document.querySelector('.hide-scrollbar')?.scrollHeight ?? -1,
    )
    expect(heightAfter).toBe(heightBefore)
    const inViewport = await page.evaluate(() => {
      const el = document.querySelector('.ui-active')
      const sc = document.querySelector('.hide-scrollbar')
      if (!el || !sc) return false
      const r = el.getBoundingClientRect()
      const c = sc.getBoundingClientRect()
      return r.top >= c.top && r.bottom <= c.bottom
    })
    expect(inViewport).toBe(true)
  })

  test('缩略图加载失败回落文本分支（图片文件被清理不滞留空白占位）', async ({ page }) => {
    await enterClipboard(page)

    // wrap 到末项滚到底：上方图片项进入 200px 预载范围，触发懒加载
    await page.keyboard.press('ArrowUp')
    await page.waitForTimeout(250)
    await expect(page.locator('.ui-active')).toHaveText(/^mock item 40/)

    // id=134（/mock/shot-35.png）的 get_clipboard_image 返回 null：
    // 失败项回落文件名文本，不滞留空白占位块
    await page.waitForTimeout(1100)
    const failedRow = page.locator('[role="option"]', { hasText: 'shot-35.png' })
    await expect(failedRow).toHaveCount(1)
    await expect(failedRow.locator('img')).toHaveCount(0)

    // 相邻图片项正常渲染缩略图（失败仅影响该条目）
    await expect(page.locator('[role="option"] img').first()).toBeVisible()
  })
})
