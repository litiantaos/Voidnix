#!/usr/bin/env node
/**
 * sync-tokens — 产品 theme.css → 官网 tokens.css token 同步
 *
 * 从 src/styles/theme.css 提取 :root / :root[data-theme="dark"] 的 CSS 自定义属性，
 * 写入 site/src/styles/tokens.css。产品改色/圆角/阴影，官网自动跟。
 *
 * 深色选择器与产品同构保留 :root[data-theme="dark"]，由 ThemeInit 内联脚本写入
 * data-theme（三态：auto 跟随系统 / light / dark，localStorage 持久化）。
 *
 * 官网专属 token（布局 / DemoStage mock / 雾团透明度补偿）在末尾 SITE_ONLY 区块，
 * 由本脚本维护——产品无对应或不适用官网场景。
 *
 * 用法：bun run sync:tokens（dev / build 前置自动调用）
 */
import { readFileSync, writeFileSync } from 'node:fs'
import { resolve, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..')
const themeSrc = readFileSync(resolve(repoRoot, 'src/styles/theme.css'), 'utf-8')

/** 提取 CSS 选择器花括号内容（保留内部注释与原始缩进）——theme.css 已 Prettier 合规，
 *  保留其缩进即可在官网 :root（同为顶层 2 空格）直接复用。pattern 容忍引号/空白差异。 */
function extractBlock(css, pattern) {
  const m = css.match(pattern)
  if (!m) throw new Error(`sync-tokens: 选择器未找到 -> ${pattern}`)
  const open = m.index + m[0].indexOf('{')
  let depth = 1
  let i = open + 1
  while (i < css.length && depth > 0) {
    if (css[i] === '{') depth++
    else if (css[i] === '}') depth--
    i++
  }
  return css
    .slice(open + 1, i - 1)
    .split('\n')
    .map((l) => l.replace(/\s+$/, '')) // 仅去行尾空白，保留行首缩进（多行值续行的额外缩进）
    .filter((l) => l.trim().length > 0) // 过滤空行
    .join('\n')
}

const lightTokens = extractBlock(themeSrc, /:root\s*\{/)
const darkTokens = extractBlock(themeSrc, /:root\[data-theme=["']dark["']\]\s*\{/)

// ── 官网专属 token（产品无对应，或需覆盖产品值）──
const SITE_ONLY_LIGHT = `  /* 字体：系统 sans 优先（拉丁走 SF Pro），CJK 仍走 PingFang SC。不继承 mono */
  --font-sans:
    -apple-system, BlinkMacSystemFont, 'SF Pro Text', 'SF Pro Display', 'Helvetica Neue',
    'Segoe UI', sans-serif, 'PingFang SC', 'Hiragino Sans GB', sans-serif;

  /* 布局（官网落地页专属，产品无） */
  --content-max: 1100px;
  --content-pad: clamp(20px, 5vw, 48px);

  /* Hero 渐变标题（产品无 accent-deep，用 accent-press 系列） */
  --color-accent-deep: color-mix(in srgb, var(--color-accent) 88%, #000);

  /* Mica 雾团：官网无 NSVisualEffect 底，透明度需高于产品 */
  --mica-fog-a: rgb(215 230 255 / 0.55);
  --mica-fog-b: rgb(205 218 245 / 0.42);

  /* DemoStage mock 场景（产品无） */
  --menubar-fill: rgb(255 255 255 / 0.45);
  --mock-input-fill: rgb(255 255 255 / 0.7);
  --mock-card-fill: rgb(255 255 255 / 0.6);
  --mock-card-hover: rgb(255 255 255 / 0.95);
  --bubble-bot-fill: rgb(248 249 252);
  --soft-card-fill: rgb(255 255 255 / 0.96);
  --shot-badge-fill: rgb(255 255 255 / 0.95);
  --shot-badge-ink: var(--color-accent);
  --color-folder: #5a8def;
  --color-file: #7aa2c8;`

const SITE_ONLY_DARK = `  color-scheme: dark;

  /* Hero 渐变标题深色提亮 */
  --color-accent-deep: color-mix(in srgb, var(--color-accent) 88%, #fff);

  /* Mica 雾团 */
  --mica-fog-a: rgb(60 80 130 / 0.34);
  --mica-fog-b: rgb(50 70 120 / 0.26);

  /* DemoStage mock 场景反相 */
  --menubar-fill: rgb(40 40 44 / 0.5);
  --mock-input-fill: rgb(44 44 48 / 0.7);
  --mock-card-fill: rgb(40 40 44 / 0.6);
  --mock-card-hover: rgb(50 50 54 / 0.95);
  --bubble-bot-fill: rgb(255 255 255 / 0.06);
  --soft-card-fill: rgb(44 44 48 / 0.96);
  --shot-badge-fill: rgb(60 60 66 / 0.92);
  --shot-badge-ink: rgb(140 175 245);
  --color-folder: #7ea5f4;
  --color-file: #9ab8d6;`

const output = `/*
 * 官网设计 token——从产品 src/styles/theme.css 自动同步（勿手改）。
 * bun run sync:tokens 重新生成。
 *
 * 深色与产品同构走 :root[data-theme="dark"]，由 ThemeInit 内联脚本写入
 * data-theme（三态：auto 跟随系统 / light / dark，localStorage 持久化）。
 * 官网专属 token（布局 / DemoStage mock / 雾团补偿）在末尾 SITE_ONLY 区块。
 */

:root {
  color-scheme: light dark;

${lightTokens}
}

:root[data-theme='dark'] {
${darkTokens}
}

/* ══ SITE_ONLY：官网专属 token（产品无对应或需覆盖）══ */
:root {
${SITE_ONLY_LIGHT}
}

/* 手动主题下原生控件配色跟随（覆盖 :root 的 light dark 双声明；auto 态由脚本解析为二值） */
:root[data-theme='light'] {
  color-scheme: light;
}

:root[data-theme='dark'] {
${SITE_ONLY_DARK}
}
`

const outPath = resolve(repoRoot, 'site/src/styles/tokens.css')
writeFileSync(outPath, output)

// 产品块为顶层 2 空格缩进，与官网选择器层级一致直接复用；但 Prettier 折行阈值
// 随上下文微变，生成后用 Prettier 二次格式化兜底；本地有根 prettier，独立部署无则
// 跳过（不影响构建，格式由本地 precommit 保证）。
try {
  const { format, resolveConfig } = await import('prettier')
  const opts = (await resolveConfig(outPath)) ?? {}
  writeFileSync(outPath, await format(output, { ...opts, filepath: outPath }))
} catch {
  // 无 prettier 环境：产物语法仍正确，格式由本地 precommit 保证
}

process.stdout.write('sync-tokens: site/src/styles/tokens.css 已同步\n')
