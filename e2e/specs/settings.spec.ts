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

  test('设置页「使用引导」回主界面（onboarded 置回）', async ({ page }) => {
    await openExtension(page, '/settings')
    await expect(page.getByText('使用引导')).toBeVisible({ timeout: 5000 })

    // 设置页禁用搜索输入（纯键盘导航）：方向键移到目标行后回车触发 action
    const input = page.locator('#main-search-input')
    const rows = page.locator('[role="option"]')
    const total = await rows.count()
    let targetIndex = -1
    for (let i = 0; i < total; i++) {
      if ((await rows.nth(i).textContent())?.includes('使用引导')) {
        targetIndex = i
        break
      }
    }
    // 目标行缺失（文案改动/被过滤）时显式失败，防盲执行第 0 行产生难定位的下游错误
    expect(targetIndex).toBeGreaterThanOrEqual(0)
    for (let j = 0; j < targetIndex; j++) await input.press('ArrowDown')
    await input.press('Enter')

    // 回主界面：设置项让位，query 清空、搜索占位恢复全局文案（引导卡本身为 Tauri-only，浏览器不渲染）
    await expect(page.locator('#main-search-input')).toHaveValue('')
    const placeholder = await page.locator('#main-search-input').getAttribute('placeholder')
    expect(placeholder).toContain('搜索应用')
  })

  test('proxy 主视图无卸载项；设置子视图空态（无核心/daemon）', async ({ page }) => {
    await openExtension(page, '/proxy')
    await expect(page.getByText('开启代理')).toBeVisible({ timeout: 5000 })
    await expect(page.getByText('规则模式')).toBeVisible()
    // 完全卸载已移至设置子视图，主列表不再出现
    await expect(page.getByText('完全卸载')).toHaveCount(0)

    // 搜索栏齿轮 → config 子视图；纯浏览器无核心状态（downloaded/daemonInstalled 均 false）→ 空态
    await page.locator('button:has(.i-ri-settings-3-line)').click()
    await page.waitForTimeout(300)
    await expect(page.getByText('未安装核心或系统组件')).toBeVisible({ timeout: 5000 })
    await expect(page.getByText('开启代理')).toHaveCount(0) // mainView 让位
    // 再点齿轮（激活态 fill 图标）返回主视图
    await page.locator('button:has(.i-ri-settings-3-fill)').click()
    await page.waitForTimeout(300)
    await expect(page.getByText('开启代理')).toBeVisible({ timeout: 5000 })
  })
})
