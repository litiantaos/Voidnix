import { test, expect } from '@playwright/test'

test.describe('Voidnix 启动器', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/')
    await page.waitForSelector('#main-search-input', { timeout: 10000 })
  })

  test('搜索框可见且可输入', async ({ page }) => {
    const input = page.locator('#main-search-input')
    await expect(input).toBeVisible()
    await input.fill('测试')
    await expect(input).toHaveValue('测试')
  })

  test('calculator 结果归入「扩展」组（module kind 归组渲染，B3）', async ({ page }) => {
    const input = page.locator('#main-search-input')
    // 算式触发 calculator 全局即时答案（base64/ip/uuid 等仅模块内响应）
    await input.fill('1+1')
    await page.waitForTimeout(300)

    const headers = page.locator('.group-header')
    // 计算结果 kind=module → 归入「扩展」组，组头渲染
    await expect(headers.filter({ hasText: '扩展' })).toBeVisible({ timeout: 5000 })
    // 计算结果可见（验证搜索→分组→渲染管道）
    await expect(page.getByText('= 2')).toBeVisible({ timeout: 5000 })
    // 纯浏览器 E2E 无原生应用索引，不命中应用组（强化归组隔离）
    await expect(headers.filter({ hasText: '应用' })).toHaveCount(0)
  })

  test('清空搜索框', async ({ page }) => {
    const input = page.locator('#main-search-input')
    await input.fill('hello')
    await input.clear()
    await expect(input).toHaveValue('')
  })

  test('输入 / 显示扩展列表', async ({ page }) => {
    const input = page.locator('#main-search-input')
    await input.fill('/')
    await page.waitForTimeout(200)
    const listItems = page.locator('[role="listbox"] > div')
    await expect(listItems.first()).toBeVisible({ timeout: 5000 })
  })

  test('搜索框占位符文本', async ({ page }) => {
    const input = page.locator('#main-search-input')
    const placeholder = await input.getAttribute('placeholder')
    expect(placeholder).toContain('搜索')
  })

  test('输入 // 显示 Google 搜索', async ({ page }) => {
    const input = page.locator('#main-search-input')
    await input.fill('//hello')
    await page.waitForTimeout(200)
    await expect(page.getByText('Google 搜索')).toBeVisible({ timeout: 3000 })
  })

  test('输入 //b 显示 Bing 搜索', async ({ page }) => {
    const input = page.locator('#main-search-input')
    await input.fill('//b hello')
    await page.waitForTimeout(200)
    await expect(page.getByText('Bing 搜索')).toBeVisible({ timeout: 3000 })
  })

  test('输入 URL 识别为链接', async ({ page }) => {
    const input = page.locator('#main-search-input')
    await input.fill('//https://example.com')
    await page.waitForTimeout(200)
    await expect(page.getByText('打开链接')).toBeVisible({ timeout: 3000 })
  })

  test('窗口重新获焦重跑不打断结果列表：选中与行保留', async ({ page }) => {
    const input = page.locator('#main-search-input')
    await input.fill('a')
    await page.waitForTimeout(300)
    const rows = page.locator('[role="option"]')
    await expect(rows.first()).toBeVisible({ timeout: 5000 })
    const countBefore = await rows.count()
    expect(countBefore).toBeGreaterThan(1)

    // 方向键下移两格：选中离开首项（回归场景：partial 替换使选中越界归零 + 列表闪变）
    await input.press('ArrowDown')
    await input.press('ArrowDown')
    const activeBefore = page.locator('.ui-active')
    await expect(activeBefore).toHaveCount(1)
    const activeTextBefore = (await activeBefore.textContent()) ?? ''

    // 模拟窗口重新获焦（Tauri 下由 onFocusChanged 派发；浏览器模式手动派发同一事件触发重跑）
    await page.evaluate(() => window.dispatchEvent(new CustomEvent('window-focused')))
    await page.waitForTimeout(500)

    // 静默重跑：列表行不变、选中不归零（重跑最终结果一致，经 keyed DOM 复用零视觉变化）
    await expect(rows).toHaveCount(countBefore)
    const activeAfter = page.locator('.ui-active')
    await expect(activeAfter).toHaveCount(1)
    expect((await activeAfter.textContent()) ?? '').toBe(activeTextBefore)
  })

  test('窗口隐藏的缓存清理不丢滚动位置（content-visibility 释放 layer 期间高度不折叠）', async ({
    page,
  }) => {
    const input = page.locator('#main-search-input')
    await input.fill('a')
    await page.waitForTimeout(300)
    const rows = page.locator('[role="option"]')
    await expect(rows.first()).toBeVisible({ timeout: 5000 })
    expect(await rows.count()).toBeGreaterThan(4)

    // 滚动列表到中部（须产生真实滚动量，列表高 > 视口内容区）
    const scrolled = await page.evaluate(() => {
      const sc = document.querySelector<HTMLElement>('.hide-scrollbar')
      if (!sc) return false
      sc.scrollTop = 100
      return sc.scrollTop > 0
    })
    expect(scrolled).toBe(true)

    // 模拟窗口隐藏（Tauri 下 hideWindow 派发；浏览器模式手动派发同一事件触发 clearCache：
    // content-visibility:hidden forced layout 释放 tile backing，DOM 与视图状态冻结保留不卸载）
    await page.evaluate(() => window.dispatchEvent(new CustomEvent('window-hiding')))
    await page.waitForTimeout(200)

    // 隐藏期间的 layer 释放不得折叠内容高度：scrollTop 被 clamp 即唤起后滚动归顶
    const topAfter = await page.evaluate(
      () => document.querySelector<HTMLElement>('.hide-scrollbar')?.scrollTop ?? -1,
    )
    expect(topAfter).toBe(100)
  })
})
