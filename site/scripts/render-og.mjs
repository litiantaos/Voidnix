// 将 og-source.html 渲染并截图为 1200×630 PNG。
// 依赖主仓库的 Playwright（node_modules/.bin/playwright + 已安装的 chromium）。
import { chromium } from 'playwright'
import { fileURLToPath } from 'node:url'
import { dirname, resolve } from 'node:path'

const here = dirname(fileURLToPath(import.meta.url))
const src = 'file://' + resolve(here, 'og-source.html')
const out = resolve(here, '../public/og-image.png')

const browser = await chromium.launch()
const page = await browser.newPage({ viewport: { width: 1200, height: 630 } })
await page.goto(src, { waitUntil: 'networkidle' })
// 等待图标字体就绪，避免方框
await page.evaluate(() => document.fonts.ready)
await page.waitForTimeout(400)
await page.screenshot({ path: out, type: 'png', clip: { x: 0, y: 0, width: 1200, height: 630 } })
await browser.close()
process.stdout.write('OG written: ' + out + '\n')
