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

  test('搜索结果按 kind 归组（base64 编码归「操作」组，B3）', async ({ page }) => {
    const input = page.locator('#main-search-input')
    await input.fill('hello')
    await page.waitForTimeout(300)
    // base64 扩展对任意输入产出编码结果（kind=module → 「操作」组）
    await expect(page.getByText('操作')).toBeVisible({ timeout: 5000 })
    // base64 编码结果可见（验证搜索→渲染管道）
    await expect(page.getByText(/aGVsbG8=/)).toBeVisible({ timeout: 5000 })
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
})
