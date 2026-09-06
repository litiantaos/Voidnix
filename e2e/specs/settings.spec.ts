import { test, expect } from '@playwright/test'

// 设置页新增「清除 Voidnix 注入」入口 + proxy 完全卸载行的门控渲染。
// 纯浏览器环境（Vite dev server）：非 Tauri 下清除动作与卸载行均不生效/不展示，
// 断言的是渲染层与状态门控（核心未下载且 daemon 未装 → 无卸载入口）。
test.describe('系统侵入面清理入口', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/')
    await page.waitForSelector('#main-search-input', { timeout: 10000 })
  })

  async function openExtension(page: import('@playwright/test').Page, keyword: string) {
    const input = page.locator('#main-search-input')
    await input.fill(keyword)
    await page.waitForTimeout(200)
    await input.press('Enter')
  }

  test('设置页「清除 Voidnix 注入」项渲染', async ({ page }) => {
    await openExtension(page, '/settings')
    await expect(page.getByText('清除 Voidnix 注入')).toBeVisible({ timeout: 5000 })
    // 副标题说明注入范围
    await expect(page.getByText('.zshrc / .zprofile 注入块与 ai.env 凭证文件')).toBeVisible()
  })

  test('proxy 视图：无核心/daemon 时完全卸载行不渲染', async ({ page }) => {
    await openExtension(page, '/proxy')
    await expect(page.getByText('开启代理')).toBeVisible({ timeout: 5000 })
    await expect(page.getByText('规则模式')).toBeVisible()
    // 纯浏览器无核心状态（downloaded=false 且 daemonInstalled=false）→ 卸载入口省略
    await expect(page.getByText('完全卸载')).toHaveCount(0)
  })
})
