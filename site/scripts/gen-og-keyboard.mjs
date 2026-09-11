// 首启引导三维键盘图纸（WelcomeView 等距态）→ 静态 SVG，作 OG 分享图右侧背景。
// 几何逐函数移植自 src/components/layout/WelcomeView.vue 的 f=0 等距终态、
// 默认键位（Alt 基）；改动键盘几何或默认键位时同步此脚本。
// 颜色读 site tokens.css（产品 theme.css 经 sync:tokens 自动同步，单一源）。
import { readFileSync, writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, resolve } from 'node:path'

const here = dirname(fileURLToPath(import.meta.url))

// ── token（tokens.css 首个 :root 块，浅色轨）──
const css = readFileSync(resolve(here, '../src/styles/tokens.css'), 'utf8')
const rootBlock = css.slice(css.indexOf(':root {') + 7, css.indexOf('\n}'))
const token = (name) => {
  const m = rootBlock.match(new RegExp(`--${name}:\\s*([^;]+);`))
  if (!m) throw new Error(`token 缺失: --${name}`)
  return m[1].trim()
}
const ACCENT = token('color-accent')
const WARNING = token('color-warning')
const BORDER = token('color-border')
const INK = token('color-text-primary')
const MONO = token('font-mono')

// ── 等距几何（轴 u=(2,KU)/v=(-2,KV)，机位反解系数与 WelcomeView 一致）──
const KU = 0.5995
const KV = 2.7569
const A = { u1: 1, ku: KU / 2, kx: -1, kxr: -1, kv: KV / 2, kvr: KV / 2, ox: 230, oy: 119, t: 8 }
const KEY_A = 31.5 // 单键半宽（横排）
const KEY_B = 17.53 // 单键半宽（纵深）
const SPACE_A = 137.5 // Space 宽板半宽
const R_TOP = 8 // 顶面角圆弧半径
const CAP_STRETCH = Math.sqrt(4 + KV * KV) / Math.sqrt(4 + KU * KU)

const vsub = (a, b) => [a[0] - b[0], a[1] - b[1]]
const vadd = (a, b) => [a[0] + b[0], a[1] + b[1]]
const vmul = (a, k) => [a[0] * k, a[1] * k]
const vnorm = (a) => {
  const l = Math.hypot(a[0], a[1]) || 1
  return [a[0] / l, a[1] / l]
}
const r2 = (n) => Math.round(n * 100) / 100
const pt = (p) => `${r2(p[0])} ${r2(p[1])}`

// 键盘网格：pos(col,row) = 原点 + col·U + row·Vrow（U 沿排 / Vrow 沿行）
const pos = (col, row) => [
  A.ox + col * 74 * A.u1 + row * 64 * A.kxr,
  A.oy + col * 74 * A.ku + row * 64 * A.kvr,
]

// 顶面四角 [前(下), 右, 后(上), 左]：角 = 中心 + p·Û + q·V̂（p=±a、q=±b）
function isoCorners(cx, cy, a, b) {
  const yF = A.ku * a + A.kv * b
  const yR = A.ku * a - A.kv * b
  const fx = (p, q) => cx + p * A.u1 + q * A.kx
  return [
    [fx(a, b), cy + yF],
    [fx(a, -b), cy + yR],
    [fx(-a, -b), cy - yF],
    [fx(-a, b), cy - yR],
  ]
}

// 角圆弧：from 侧距 corner r 处入弧，Q 过 corner，至 to 侧距 r 处出弧
function cornerArc(from, corner, to, r) {
  const pin = vsub(corner, vmul(vnorm(vsub(corner, from)), r))
  const pout = vadd(corner, vmul(vnorm(vsub(to, corner)), r))
  return { pin, pout, d: `L ${pt(pin)} Q ${pt(corner)} ${pt(pout)}` }
}

// 二次弧 de Casteljau 半分割：侧壁弧段与顶面共享控制点全程共线
function arcSplit(pin, c, pout) {
  return {
    mid: [(pin[0] + 2 * c[0] + pout[0]) / 4, (pin[1] + 2 * c[1] + pout[1]) / 4],
    m1: [(pin[0] + c[0]) / 2, (pin[1] + c[1]) / 2],
    m2: [(c[0] + pout[0]) / 2, (c[1] + pout[1]) / 2],
  }
}

// 板的三面：顶面 + 左右壁 + 顶面高光棱线（壁厚 t 竖直下沉）
function plate(cx, cy, a, b, t) {
  const [front, right, back, left] = isoCorners(cx, cy, a, b)
  const aF = cornerArc(left, front, right, R_TOP)
  const aR = cornerArc(front, right, back, R_TOP)
  const aB = cornerArc(right, back, left, R_TOP)
  const aL = cornerArc(back, left, front, R_TOP)
  const top = `M ${pt(aL.pin)} Q ${pt(left)} ${pt(aL.pout)} ${aF.d} ${aR.d} ${aB.d} Z`
  const f = arcSplit(aF.pin, front, aF.pout)
  const l = arcSplit(aL.pin, left, aL.pout)
  const rc = arcSplit(aR.pin, right, aR.pout)
  const drop = (p) => [p[0], p[1] + t]
  const wallL =
    `M ${pt(l.mid)} Q ${pt(l.m2)} ${pt(aL.pout)} L ${pt(aF.pin)} Q ${pt(f.m1)} ${pt(f.mid)} ` +
    `L ${pt(drop(f.mid))} Q ${pt(drop(f.m1))} ${pt(drop(aF.pin))} L ${pt(drop(aL.pout))} ` +
    `Q ${pt(drop(l.m2))} ${pt(drop(l.mid))} Z`
  const wallR =
    `M ${pt(f.mid)} Q ${pt(f.m2)} ${pt(aF.pout)} L ${pt(aR.pin)} Q ${pt(rc.m1)} ${pt(rc.mid)} ` +
    `L ${pt(drop(rc.mid))} Q ${pt(drop(rc.m1))} ${pt(drop(aR.pin))} L ${pt(drop(aF.pout))} ` +
    `Q ${pt(drop(f.m2))} ${pt(drop(f.mid))} Z`
  const sheen = `M ${pt(aR.pout)} L ${pt(aB.pin)} Q ${pt(back)} ${pt(aB.pout)} L ${pt(aL.pin)}`
  return { top, wallL, wallR, sheen }
}

// 键名贴面矩阵（等距态）：x 列按 u/v 轴投影长度比拉伸补偿横向压扁
const capXform = (cx, cy) =>
  `matrix(${CAP_STRETCH.toFixed(4)} ${(CAP_STRETCH * A.ku).toFixed(4)} ${A.kx.toFixed(4)} ${A.kv.toFixed(4)} ${r2(cx)} ${r2(cy)})`

// 板阵（默认键位 Alt 基，painter 序平面 row/col 升序）。tone 分层与 app 同款：
// mod 修饰键琥珀 / main 主快捷键动作键重笔 / ghost ⌘ 占位弱化 / fn 功能键常态
const SPECS = [
  { id: 'translate', col: 4, row: -1, tone: 'fn', symbol: 'T' },
  { id: 'agent', col: 0, row: 0, tone: 'fn', symbol: 'A' },
  { id: 'screenshot', col: 1, row: 0, tone: 'fn', symbol: 'S' },
  { id: 'finder-ext', col: 3, row: 0, tone: 'fn', symbol: 'F' },
  { id: 'clipboard', col: 2, row: 1, tone: 'fn', symbol: 'C' },
  { id: 'notes', col: 4, row: 1, tone: 'fn', symbol: 'N' },
  { id: 'slash', col: 6, row: 1, tone: 'fn', symbol: '/' },
  { id: 'mod0', col: 0, row: 2, tone: 'mod', symbol: '⌥' },
  { id: 'ghost1', col: 1, row: 2, tone: 'ghost', symbol: '⌘' },
  { id: 'space', col: 3.4324, row: 2, tone: 'main', symbol: 'Space' },
]
  .sort((x, y) => x.row - y.row || x.col - y.col)
  .map((k) => ({ ...k, a: k.id === 'space' ? SPACE_A : KEY_A, b: KEY_B }))

// 网格线：贴键底缘的横向线族（沿 u 轴），板 u 占位内几何断开；全部段合并单 path
const GRID_STEP = 74
const GRID_U_MIN = -1
const GRID_X_MAX = 714
const GRID_MARGIN = 0.2

function gridPath(specs) {
  const drop = (p) => [p[0], p[1] + A.t]
  const line = (p, q) => `M ${pt(p)} L ${pt(q)}`
  const cuts = new Map()
  for (const k of specs) {
    const half = k.a / GRID_STEP
    for (const side of [-1, 1]) {
      const v = k.row + (side * k.b * A.kv) / -(64 * A.kvr)
      const key = Math.round(v * 1e3)
      const seg = [k.col - half * A.u1, k.col + half * A.u1]
      const arr = cuts.get(key)
      if (arr) arr.push(seg)
      else cuts.set(key, [seg])
    }
  }
  const uEndOf = (list, v) => {
    const hi = Math.max(...list.map(([, h]) => h))
    if (v >= 1.5) return hi
    return Math.min(hi + GRID_MARGIN, (GRID_X_MAX - A.ox - 64 * A.kxr * v) / (74 * A.u1))
  }
  const uStart = (v) =>
    Math.max(
      GRID_U_MIN,
      (6 - A.ox - 64 * A.kxr * v) / (74 * A.u1),
      ...(A.ku > 0.05 ? [(6 - A.oy - 64 * A.kvr * v) / (74 * A.ku)] : []),
    )
  const out = []
  for (const [vk, list] of cuts) {
    const v = vk / 1e3
    const uEnd = uEndOf(list, v)
    list.sort((a, b) => a[0] - b[0])
    let u = uStart(v)
    for (const [lo, hi] of list) {
      if (lo > u) out.push(line(drop(pos(u, v)), drop(pos(Math.min(lo, uEnd), v))))
      if (hi > u) u = hi
    }
    if (u < uEnd) out.push(line(drop(pos(u, v)), drop(pos(uEnd, v))))
  }
  return out.join(' ')
}

// ── 组装 SVG（样式为 WelcomeView 浅色轨的烘焙版）──
const style = `text{pointer-events:none}
.w-grid{fill:none;stroke:${BORDER};stroke-width:1;stroke-dasharray:6 8;stroke-linecap:round}
.w-wall-l,.w-wall-r{stroke:${ACCENT};stroke-opacity:.5;stroke-width:1.25;stroke-linejoin:round}
.w-wall-l{fill:url(#w-hatch-l)}
.w-wall-r{fill:url(#w-hatch-r)}
.w-top{fill:url(#w-top-grad);fill-opacity:.55;stroke:${ACCENT};stroke-opacity:.5;stroke-width:1.25;stroke-linejoin:round}
.w-plate-main .w-top{fill-opacity:1;stroke-opacity:.9}
.w-plate-main .w-wall-l,.w-plate-main .w-wall-r{stroke-opacity:.85}
.w-plate-mod .w-top{fill:url(#w-top-grad-mod);fill-opacity:1;stroke:${WARNING};stroke-opacity:.9}
.w-plate-mod .w-wall-l,.w-plate-mod .w-wall-r{stroke:${WARNING};stroke-opacity:.85}
.w-plate-mod .w-wall-l{fill:url(#w-hatch-l-mod)}
.w-plate-mod .w-wall-r{fill:url(#w-hatch-r-mod)}
.w-plate-ghost .w-top{fill-opacity:.22;stroke-opacity:.35}
.w-plate-ghost .w-wall-l,.w-plate-ghost .w-wall-r{stroke-opacity:.35}
.w-plate-ghost .w-cap{fill:${ACCENT};opacity:.55}
.w-sheen{fill:none;stroke:${ACCENT};stroke-opacity:.22;stroke-width:1;stroke-linecap:round}
.w-plate-main .w-sheen{stroke-opacity:.4}
.w-plate-mod .w-sheen{stroke:${WARNING};stroke-opacity:.4}
.w-plate-ghost .w-sheen{opacity:0}
.w-cap{fill:${INK};font-family:${MONO};font-size:7px;font-weight:500;text-anchor:middle}`

const defs = `
    <pattern id="w-hatch-l" width="6" height="6" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
      <line x1="0" y1="0" x2="0" y2="6" stroke="${ACCENT}" stroke-opacity="0.4" stroke-width="1" />
    </pattern>
    <pattern id="w-hatch-r" width="5" height="5" patternUnits="userSpaceOnUse" patternTransform="rotate(20)">
      <line x1="0" y1="0" x2="0" y2="5" stroke="${ACCENT}" stroke-opacity="0.4" stroke-width="1" />
    </pattern>
    <linearGradient id="w-top-grad" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="${ACCENT}" stop-opacity="0.12" />
      <stop offset="1" stop-color="${ACCENT}" stop-opacity="0.045" />
    </linearGradient>
    <pattern id="w-hatch-l-mod" width="6" height="6" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
      <line x1="0" y1="0" x2="0" y2="6" stroke="${WARNING}" stroke-opacity="0.4" stroke-width="1" />
    </pattern>
    <pattern id="w-hatch-r-mod" width="5" height="5" patternUnits="userSpaceOnUse" patternTransform="rotate(20)">
      <line x1="0" y1="0" x2="0" y2="5" stroke="${WARNING}" stroke-opacity="0.4" stroke-width="1" />
    </pattern>
    <linearGradient id="w-top-grad-mod" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="${WARNING}" stop-opacity="0.12" />
      <stop offset="1" stop-color="${WARNING}" stop-opacity="0.045" />
    </linearGradient>`

const plates = SPECS.map((k) => {
  const [cx, cy] = pos(k.col, k.row)
  const geom = plate(cx, cy, k.a, k.b, A.t)
  return `  <g class="w-plate w-plate-${k.tone}">
    <path class="w-wall-l" d="${geom.wallL}" />
    <path class="w-wall-r" d="${geom.wallR}" />
    <path class="w-top" d="${geom.top}" />
    <path class="w-sheen" d="${geom.sheen}" />
    <text class="w-cap" transform="${capXform(cx, cy)}" x="0" y="1.5">${k.symbol}</text>
  </g>`
}).join('\n')

const svg = `<!-- 由 gen-og-keyboard.mjs 生成（勿手改）：首启引导三维键盘图纸等距态 -->
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 720 480" role="img" aria-label="Voidnix 快捷键键位图">
  <style>${style}</style>
  <defs>${defs}
  </defs>
  <path class="w-grid" d="${gridPath(SPECS)}" />
${plates}
</svg>
`

const out = resolve(here, 'og-keyboard.svg')
writeFileSync(out, svg)
process.stdout.write(`keyboard svg written: ${out} (${SPECS.length} plates)\n`)
