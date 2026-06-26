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

  test('base64 结果归入「扩展」组（module kind 归组渲染，B3）', async ({ page }) => {
    const input = page.locator('#main-search-input')
    // 合法 base64 串（= "hello"）触发全局模式解码分支（编码分支仅模块内生效）
    await input.fill('aGVsbG8=')
    await page.waitForTimeout(300)

    const headers = page.locator('.group-header')
    // 解码结果 kind=module → 归入「扩展」组，组头渲染
    await expect(headers.filter({ hasText: '扩展' })).toBeVisible({ timeout: 5000 })
    // 解码结果可见（验证搜索→分组→渲染管道）
    await expect(page.getByText('Base64 解码')).toBeVisible({ timeout: 5000 })
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
})
