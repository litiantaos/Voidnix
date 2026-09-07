import { test, expect } from '@playwright/test'

// agent 未配置空态 → 直达 ai-providers 配置（纯浏览器环境 ai-providers 恒为空，空态稳定可见）。
test.describe('agent 未配置引导', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/')
    await page.waitForSelector('#main-search-input', { timeout: 10000 })
  })

  test('未配置空态按钮直达 AI 提供商配置', async ({ page }) => {
    const input = page.locator('#main-search-input')
    await input.fill('/agent')
    await page.waitForTimeout(200)
    await input.press('Enter')

    // 未配置空态：提示 + 直达按钮
    await expect(page.getByText('请先配置 AI 提供商')).toBeVisible({ timeout: 5000 })
    const goConfig = page.getByRole('button', { name: '去配置 AI 提供商' })
    await expect(goConfig).toBeVisible()
    await goConfig.click()

    // 切到 ai-providers 扩展：空态（纯浏览器无凭证）
    await expect(page.getByText('请添加 AI 提供商')).toBeVisible({ timeout: 5000 })
  })
})
