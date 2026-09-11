// 将 og-source.html 渲染并截图为 1200×630 PNG（zh / en 双版本）。
// 中文版即源稿原样；英文版渲染前注入文案替换（与 site i18n hero 对齐），
// 布局/键盘背景共用同一源（信息行两行纵排为源稿基础；英文标题两行由注入的 <br> 承担）。
// playwright 是 site 自身 devDependency，另需 chromium。
import { chromium } from 'playwright'
import { fileURLToPath } from 'node:url'
import { dirname, resolve } from 'node:path'

const here = dirname(fileURLToPath(import.meta.url))
const src = 'file://' + resolve(here, 'og-source.html')

// 英文版文案（与 site i18n hero 对齐）；信息行两行纵排为源稿基础布局
const EN = {
  title: 'From thought<br /><em>to action.</em>',
  desc: 'macOS Productivity Launcher',
  feats: 'Clipboard · Screenshot · Translate · AI Assistant · Window Manager & more',
}

const browser = await chromium.launch()
const page = await browser.newPage({ viewport: { width: 1200, height: 630 } })
await page.goto(src, { waitUntil: 'networkidle' })
// 等待图标字体就绪，避免方框
await page.evaluate(() => document.fonts.ready)
await page.waitForTimeout(400)

const shot = (name) =>
  page.screenshot({
    path: resolve(here, `../public/${name}`),
    type: 'png',
    clip: { x: 0, y: 0, width: 1200, height: 630 },
  })

await shot('og-image.png')

await page.evaluate((en) => {
  document.documentElement.lang = 'en'
  document.querySelector('.title').innerHTML = en.title
  document.querySelector('.info .desc').textContent = en.desc
  document.querySelector('.info .feats').textContent = en.feats
}, EN)
await page.waitForTimeout(200)
await shot('og-image-en.png')

await browser.close()
process.stdout.write('OG written: og-image.png + og-image-en.png\n')
