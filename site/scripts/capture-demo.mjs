// ═══════════════════════════════════════════════
//  Voidnix 宣传片捕获脚本
//  Playwright 逐帧截图 → ffmpeg 编码 MP4 + WebM
// ═══════════════════════════════════════════════
import { spawn, execSync } from 'node:child_process'
import { chromium } from 'playwright'
import fs from 'node:fs'
import path from 'node:path'
import os from 'node:os'
import { fileURLToPath } from 'node:url'

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const SITE_DIR = path.resolve(__dirname, '..')

// ── 配置 ──
const FPS = 30
const TOTAL_FRAMES = 840
const WIDTH = 1280
const HEIGHT = 720
const SCALE = 2 // retina 渲染（截图 2560×1440 → 编码降采样更锐利）
const PORT = 4399

// ── 工具 ──
function log(msg) {
  process.stdout.write(msg + '\n')
}

async function waitForUrl(url, timeout = 30000) {
  const start = Date.now()
  while (Date.now() - start < timeout) {
    try {
      const res = await fetch(url)
      if (res.ok) return
    } catch {
      // server not ready yet
    }
    await new Promise((r) => setTimeout(r, 500))
  }
  throw new Error(`Dev server 未在 ${timeout}ms 内就绪（${url}）`)
}

async function main() {
  // ── 启动 Astro dev server ──
  log('启动 Astro dev server...')
  const server = spawn('bun', ['run', 'dev', '--port', String(PORT)], {
    cwd: SITE_DIR,
    stdio: ['ignore', 'pipe', 'pipe'],
    detached: true, // 子进程独立进程组，finally 用 process.kill(-pid) 回收整棵子树
  })

  let browser
  let tmpDir
  try {
    await waitForUrl(`http://localhost:${PORT}/demo`, 30000)
    log('Dev server 就绪')

    // ── 启动浏览器 ──
    browser = await chromium.launch()
    const context = await browser.newContext({
      viewport: { width: WIDTH, height: HEIGHT },
      deviceScaleFactor: SCALE,
      colorScheme: 'light',
    })
    const page = await context.newPage()

    // ── 导航到 demo 页（捕获模式） ──
    await page.goto(`http://localhost:${PORT}/demo?capture=1`, {
      waitUntil: 'domcontentloaded',
    })

    // 等待字体加载（系统字体 + Remixicon CDN）
    await page.evaluate(() => document.fonts.ready)
    await new Promise((r) => setTimeout(r, 800))

    // ── 逐帧截图 ──
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'voidnix-demo-'))
    log(`捕获 ${TOTAL_FRAMES} 帧 → ${tmpDir}`)

    for (let f = 0; f < TOTAL_FRAMES; f++) {
      await page.evaluate((frame) => window.__renderFrame(frame), f)
      await page.screenshot({
        path: path.join(tmpDir, `${String(f).padStart(5, '0')}.png`),
        type: 'png',
      })
      if (f % 30 === 0 || f === TOTAL_FRAMES - 1) {
        const pct = Math.round(((f + 1) / TOTAL_FRAMES) * 100)
        process.stdout.write(`\r  ${f + 1}/${TOTAL_FRAMES} (${pct}%)`)
      }
    }
    process.stdout.write('\n')
    log('帧捕获完成')

    await browser.close()
    browser = null

    // ── ffmpeg 编码 ──
    const publicDir = path.join(SITE_DIR, 'public')
    const mp4Path = path.join(publicDir, 'demo.mp4')
    const webmPath = path.join(publicDir, 'demo.webm')
    const framePattern = path.join(tmpDir, '%05d.png')

    log('编码 MP4（H.264）...')
    execSync(
      `ffmpeg -y -framerate ${FPS} -i "${framePattern}" ` +
        `-c:v libx264 -preset slow -crf 18 -pix_fmt yuv420p ` +
        `-vf scale=${WIDTH}:${HEIGHT} -movflags +faststart "${mp4Path}"`,
      { stdio: 'inherit' },
    )

    log('编码 WebM（VP9）...')
    execSync(
      `ffmpeg -y -i "${mp4Path}" ` +
        `-c:v libvpx-vp9 -crf 30 -b:v 0 ` +
        `-vf scale=${WIDTH}:${HEIGHT} "${webmPath}"`,
      { stdio: 'inherit' },
    )

    // ── 清理临时帧 ──
    fs.rmSync(tmpDir, { recursive: true, force: true })
    tmpDir = null

    // ── 报告 ──
    const mp4MB = (fs.statSync(mp4Path).size / 1024 / 1024).toFixed(1)
    const webmMB = (fs.statSync(webmPath).size / 1024 / 1024).toFixed(1)
    log(`完成！`)
    log(`  MP4:  ${mp4Path} (${mp4MB} MB)`)
    log(`  WebM: ${webmPath} (${webmMB} MB)`)
  } finally {
    if (browser) await browser.close().catch(() => {})
    if (tmpDir) fs.rmSync(tmpDir, { recursive: true, force: true })
    // 进程组信号回收 dev server 整棵子树（bun → astro → vite），避免端口残留
    try {
      process.kill(-server.pid, 'SIGTERM')
    } catch {
      server.kill('SIGTERM')
    }
  }
}

main().catch((err) => {
  console.error('捕获失败:', err)
  process.exit(1)
})
