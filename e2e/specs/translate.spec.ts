import { test, expect } from '@playwright/test'

// translate 未配置空态（纯浏览器环境 youdao/AI 均无凭证）→ 统一「去配置」进入自身设置子视图。
test.describe('translate 未配置引导', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/')
    await page.waitForSelector('#main-search-input', { timeout: 10000 })
  })

  test('未配置空态按钮进入翻译设置', async ({ page }) => {
    const input = page.locator('#main-search-input')
    await input.fill('/translate')
    await page.waitForTimeout(200)
    await input.press('Enter')

    // 未配置空态：提示 + 统一「去配置」按钮（BaseSetupState）
    await expect(page.getByText('请先配置翻译服务')).toBeVisible({ timeout: 5000 })
    const goConfig = page.getByRole('button', { name: '去配置' })
    await expect(goConfig).toBeVisible()
    await goConfig.click()

    // 进入 translate 设置子视图：「翻译服务」分组（有道 / AI 两项）
    await expect(page.getByText('翻译服务', { exact: true })).toBeVisible({ timeout: 5000 })
    await expect(page.getByText('有道翻译')).toBeVisible()
  })
})
