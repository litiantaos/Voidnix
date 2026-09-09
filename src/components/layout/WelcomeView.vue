<template>
  <div class="welcome-view">
    <!-- 主体：键盘键位图（等距图纸）——纯信息展示零交互，描述全部经引出线标注 -->
    <div class="w-stage w-step">
      <svg class="w-iso" viewBox="0 0 720 480" role="img" :aria-label="stageAria">
        <!-- 机位：方位角 25°（右前方）/ 俯角 40° / 歪头 0°——俯仰方位由 KU/KV 表达（见 script） -->
        <defs>
          <!-- 剖切填充：均右上向——左壁 45°、线距 6；右壁 70°、线距 5（其缘线陡升 54°，
               45° 近并行显平，提陡避开并稍加密） -->
          <pattern
            id="w-hatch-l"
            width="6"
            height="6"
            patternUnits="userSpaceOnUse"
            patternTransform="rotate(45)"
          >
            <line
              x1="0"
              y1="0"
              x2="0"
              y2="6"
              stroke="var(--color-accent)"
              stroke-opacity="0.4"
              stroke-width="1"
            />
          </pattern>
          <pattern
            id="w-hatch-r"
            width="5"
            height="5"
            patternUnits="userSpaceOnUse"
            patternTransform="rotate(20)"
          >
            <line
              x1="0"
              y1="0"
              x2="0"
              y2="5"
              stroke="var(--color-accent)"
              stroke-opacity="0.4"
              stroke-width="1"
            />
          </pattern>
          <!-- 顶面：accent 浅染渐变（板面微立体；强度供 tone 分层 fill-opacity 调制） -->
          <linearGradient id="w-top-grad" x1="0" y1="0" x2="1" y2="1">
            <stop offset="0" stop-color="var(--color-accent)" stop-opacity="0.12" />
            <stop offset="1" stop-color="var(--color-accent)" stop-opacity="0.045" />
          </linearGradient>
        </defs>

        <!-- 等距网格线（板下层，穿过板面的部分被板覆盖） -->
        <path v-for="(d, i) in gridLines" :key="`grid-${i}`" class="w-grid" :d="d" />

        <!-- 键帽板（painter 序：平面 row/col 升序，近者后画覆盖远者的壁）。
             tone 分层：main 主快捷键重笔 / fn 功能键常态 / ghost ⌘ 占位弱化 -->
        <g
          v-for="k in allPlates"
          :key="k.id"
          class="w-plate"
          :class="`w-plate-${k.tone} w-key-in`"
          :style="{ animationDelay: `${200 + k.i * 80}ms` }"
        >
          <path class="w-wall-l" :d="k.geom.wallL" />
          <path class="w-wall-r" :d="k.geom.wallR" />
          <path class="w-top" :d="k.geom.top" />
          <path class="w-sheen" :d="k.geom.sheen" />
          <!-- 键名贴面：本地 x→u 轴（沿排阅读）、y→v 轴（竖笔平行 v 边），随板透视 -->
          <text class="w-cap" :transform="capXform(k.cx, k.cy)" x="0" y="1.5">{{ k.symbol }}</text>
        </g>

        <!-- 修饰键引出线：端点在 col0 修饰板远侧边中心，文字对齐水平段左端（左拉）。
             与其余引出线同经 callout() 计算；单键组合无修饰键不渲染 -->
        <g v-if="modifierCallout" class="w-callout w-fade" :style="{ animationDelay: '420ms' }">
          <circle :cx="modifierCallout.dot[0]" :cy="modifierCallout.dot[1]" r="3" />
          <path :d="modifierCallout.d" />
          <text
            class="w-co-main"
            :x="modifierCallout.lx"
            :y="modifierCallout.ly"
            :text-anchor="modifierCallout.anchor"
          >
            {{ t('welcome.isoModifier') }}
          </text>
        </g>

        <!-- 功能键引出线：全部经线拉出（端点 + 虚线 + 词） -->
        <g
          v-for="(fn, fi) in fnItems"
          :key="fn.id + '-co'"
          class="w-callout w-fade"
          :style="{ animationDelay: `${470 + fi * 60}ms` }"
        >
          <circle :cx="fn.co.dot[0]" :cy="fn.co.dot[1]" r="3" />
          <path :d="fn.co.d" />
          <text class="w-co-main" :x="fn.co.lx" :y="fn.co.ly" :text-anchor="fn.co.anchor">
            {{ t(fn.labelKey) }}
          </text>
        </g>

        <!-- Space（动作宽板）引出线：端点在宽板右侧边中点沿边法向外移 12px（正对边
             中心、脱出壁带），斜段下行右折（转折开口向上），文字对齐水平段右端（右拉） -->
        <g v-if="spaceCallout" class="w-callout w-fade" :style="{ animationDelay: '560ms' }">
          <circle :cx="spaceCallout.dot[0]" :cy="spaceCallout.dot[1]" r="3" />
          <path :d="spaceCallout.d" />
          <text
            class="w-co-main"
            :x="spaceCallout.lx"
            :y="spaceCallout.ly"
            :text-anchor="spaceCallout.anchor"
          >
            {{ t('welcome.isoAction') }}
          </text>
        </g>
        <!-- 「/」语法键引出线：动词「输入」与快捷键的名词标注区分（非组合键）；
             双句两行右对齐（行距 13），控宽不横跨半幅图纸 -->
        <g class="w-callout w-fade" :style="{ animationDelay: '680ms' }">
          <circle :cx="slashCallout.dot[0]" :cy="slashCallout.dot[1]" r="3" />
          <path :d="slashCallout.d" />
          <text
            class="w-co-main"
            :x="slashCallout.lx"
            :y="slashCallout.ly - 13"
            :text-anchor="slashCallout.anchor"
          >
            {{ t('welcome.noteTools') }}
          </text>
          <text
            class="w-co-main"
            :x="slashCallout.lx"
            :y="slashCallout.ly"
            :text-anchor="slashCallout.anchor"
          >
            {{ t('welcome.noteSearch') }}
          </text>
        </g>
        <!-- 图签：左下角应用名（入场序列末位，随标注之后淡入）+ 品牌口号（名字下方，
             底部对齐板阵最低点）。同宽约束仅 zh：CJK 等宽字形无感；en 词形长，
             压同宽会挤字距，保持自然宽左对齐 -->
        <text
          class="w-title w-fade"
          x="53"
          y="423"
          textLength="96"
          :style="{ animationDelay: '660ms' }"
        >
          Voidnix
        </text>
        <text
          class="w-tagline w-fade"
          x="53"
          y="443"
          :textLength="taglineLength"
          :style="{ animationDelay: '660ms' }"
        >
          {{ t('welcome.tagline') }}
        </text>
      </svg>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { onKeyStroke } from '@/composables/events'
import { useSettingsStore } from '@/stores/settings'
import { useAppStore } from '@/stores/app'
import { formatShortcutKeys } from '@/utils/format'
import { getExtension } from '@/runtime/extension-registry'
import { t, locale } from '@/runtime/i18n'

/// 首启引导（fullscreen 槽供给方，MainView 整窗层渲染）：等距键位图，纯信息展示零交互。
/// 槽契约：Enter/Esc 完结——自持落盘 onboarded 后 emit done 仅请求框架收尾
///（框架键盘已让位，组件自治按键）。
const emit = defineEmits<{ done: [] }>()

const settings = useSettingsStore()
const appStore = useAppStore()

const shortcutKeys = computed(() => formatShortcutKeys(settings.globalShortcut))

/// 口号同宽约束仅 zh（CJK 等宽字形分配无感）；en 词形长，压到 96 会挤字距
const taglineLength = computed(() => (locale.value === 'zh-CN' ? 96 : undefined))

onKeyStroke(['Enter', 'Escape'], (e) => {
  // Enter 焦点在按钮上时让位给按钮自身激活（BaseList 同款守卫），防双发
  if (e.key === 'Enter' && document.activeElement?.tagName === 'BUTTON') return
  requestDone()
})

/// 完结双重守卫：一次性（重复 Enter/Esc 不二次 emit）且仅在槽仍激活时有效——
/// 让位后收到的按键不完结不落盘（done 会被框架入口守卫忽略，先行落盘会让
/// 引导未经确认即永久跳过）
let finished = false

function requestDone() {
  if (finished || appStore.fullscreenView === null) return
  finished = true
  settings.onboarded = true
  emit('done')
}

// ── 等距几何（轴 u=(2,KU) / v=(-2,KV)，a/b 为沿轴半宽的屏幕 x 分量）────────
// KU/KV 为轴 y 分量系数，由机位反解（θ=25° 右前方、俯角 40°）：u 坡 16.7°/v 坡
// 54.0°。推导与取舍（轴不对称容忍）见 AGENTS.md
const KU = 0.5995
const KV = 2.7569
type Pt = [number, number]

const vsub = (a: Pt, b: Pt): Pt => [a[0] - b[0], a[1] - b[1]]
const vadd = (a: Pt, b: Pt): Pt => [a[0] + b[0], a[1] + b[1]]
const vmul = (a: Pt, k: number): Pt => [a[0] * k, a[1] * k]
const vnorm = (a: Pt): Pt => {
  const l = Math.hypot(a[0], a[1]) || 1
  return [a[0] / l, a[1] / l]
}
const pt = (p: Pt): string => `${p[0]} ${p[1]}`

/// 键名贴面矩阵：平面 (P,Q) → 屏幕 x=P−Q、y=(KU·P+KV·Q)/2，即 x̂→u 轴 (1,KU/2)、
/// ŷ→v 轴 (−1,KV/2)——阅读线沿排（u 坡），竖笔与 v 边平行（54° 右上倾）。
/// u/v 轴投影长度不对称（√(4+KU²)≈2.09 vs √(4+KV²)≈3.41）会把字面横向压扁，
/// x 列按两轴长度比拉伸补偿，恢复字形本征纵横比
const CAP_STRETCH = Math.sqrt(4 + KV * KV) / Math.sqrt(4 + KU * KU)
const capXform = (cx: number, cy: number) =>
  `matrix(${CAP_STRETCH} ${(CAP_STRETCH * KU) / 2} -1 ${KV / 2} ${cx} ${cy})`

function isoCorners(cx: number, cy: number, a: number, b: number): Pt[] {
  // 返回 [前(下), 右, 后(上), 左] 四角（back/left 的 y 分量取负：两轴均带 +y 分量）
  const yF = (KU * a + KV * b) / 2
  const yR = (KU * a - KV * b) / 2
  return [
    [cx + a - b, cy + yF],
    [cx + a + b, cy + yR],
    [cx - a + b, cy - yF],
    [cx - a - b, cy - yR],
  ]
}

/// 角圆弧：from 侧距 corner r 处入弧，Q 过 corner，至 to 侧距 r 处出弧
function cornerArc(from: Pt, corner: Pt, to: Pt, r: number) {
  const pin = vsub(corner, vmul(vnorm(vsub(corner, from)), r))
  const pout = vadd(corner, vmul(vnorm(vsub(to, corner)), r))
  return { pin, pout, d: `L ${pt(pin)} Q ${pt(corner)} ${pt(pout)}` }
}

/// 二次弧 de Casteljau 半分割：mid = 弧上 t=0.5 点（对称弧的极值点，切线竖直/沿轴），
/// m1 / m2 = 前半段与后半段的控制点——侧壁沿弧分割段与顶面共线必须用同一套点
function arcSplit(pin: Pt, c: Pt, pout: Pt) {
  const mid: Pt = [(pin[0] + 2 * c[0] + pout[0]) / 4, (pin[1] + 2 * c[1] + pout[1]) / 4]
  const m1: Pt = [(pin[0] + c[0]) / 2, (pin[1] + c[1]) / 2]
  const m2: Pt = [(c[0] + pout[0]) / 2, (c[1] + pout[1]) / 2]
  return { mid, m1, m2 }
}

/** 顶面角圆弧半径（屏幕像素，全部板共用） */
const R_TOP = 8

/**
 * 板的三面：顶面 + 左右壁 + 顶面高光。
 * 侧壁可见范围 = 顶面轮廓上外法线朝下的部分：从左弧竖直切点（x 极值）经前角弧
 * 至右弧竖直切点，端边收成竖直厚度线，与顶面圆弧全程共线（弧段经 de Casteljau
 * 分割共享控制点）；左右壁分界棱线在前角弧中点，底边与顶边逐段平行。
 */
function plate(cx: number, cy: number, a: number, b: number, t: number) {
  const [front, right, back, left] = isoCorners(cx, cy, a, b)
  const aF = cornerArc(left!, front!, right!, R_TOP)
  const aR = cornerArc(front!, right!, back!, R_TOP)
  const aB = cornerArc(right!, back!, left!, R_TOP)
  const aL = cornerArc(back!, left!, front!, R_TOP)
  const top = `M ${pt(aL.pin)} Q ${pt(left!)} ${pt(aL.pout)} ${aF.d} ${aR.d} ${aB.d} Z`
  const f = arcSplit(aF.pin, front!, aF.pout)
  const l = arcSplit(aL.pin, left!, aL.pout)
  const r = arcSplit(aR.pin, right!, aR.pout)
  const drop = (p: Pt): Pt => [p[0], p[1] + t]
  // 左壁：左弧切点 → 弧后半 → 前边 → 前弧前半 → 弧中点 → 竖直下沿 → 底部反走至左切点闭合
  const wallL =
    `M ${pt(l.mid)} Q ${pt(l.m2)} ${pt(aL.pout)} L ${pt(aF.pin)} Q ${pt(f.m1)} ${pt(f.mid)} ` +
    `L ${pt(drop(f.mid))} Q ${pt(drop(f.m1))} ${pt(drop(aF.pin))} L ${pt(drop(aL.pout))} ` +
    `Q ${pt(drop(l.m2))} ${pt(drop(l.mid))} Z`
  // 右壁：前弧中点 → 弧后半 → 前右边 → 右弧前半 → 切点 → 竖直下沿 →
  // 底部反走（右弧前半 + 前右边 + 前弧后半，与顶边逐段平行）→ 弧中点闭合
  const wallR =
    `M ${pt(f.mid)} Q ${pt(f.m2)} ${pt(aF.pout)} L ${pt(aR.pin)} Q ${pt(r.m1)} ${pt(r.mid)} ` +
    `L ${pt(drop(r.mid))} Q ${pt(drop(r.m1))} ${pt(drop(aR.pin))} L ${pt(drop(aF.pout))} ` +
    `Q ${pt(drop(f.m2))} ${pt(drop(f.mid))} Z`
  // 顶面高光：上缘两条边 + 后角圆弧（开放细线）
  const sheen = `M ${pt(aR.pout)} L ${pt(aB.pin)} Q ${pt(back!)} ${pt(aB.pout)} L ${pt(aL.pin)}`
  return { top, wallL, wallR, sheen }
}

// ── 键盘网格：pos(col, row) = 原点 + col·U + row·V（U 沿排 / V 沿行，步进含缝）──
// 平面网格矩形化：横排键距 74、排距 64（≈0.86 键距同真实键盘）。排间为整体
// 平移（C 对 S、N 对 F 的偏移同为 U+V = 10px），勿按屏幕垂直线对齐个别键
const O: Pt = [230, 119]
const U: Pt = [74, (74 * KU) / 2]
const V: Pt = [-64, (64 * KV) / 2]
const pos = (col: number, row: number): Pt => [
  O[0] + col * U[0] + row * V[0],
  O[1] + col * U[1] + row * V[1],
]

/** 板厚（竖直挤出 t，屏幕 y 分量）；网格线对齐柱体底面（顶面缘下沉 t） */
const PLATE_T = 8

/** 单键半宽：a 沿横排 / b 沿纵深（屏幕 x 分量）。b 按等边条件
 * b·√(4+KV²) = 28.59·√(4+KU²) 取值，a 在其上加大——横边（u 边）65.8 / 纵边
 *（v 边）59.7，边比 1.10（水平方向的边更长，键帽横长观感）；⌘ 与单键同尺寸 */
const KEY_B = 17.53
const KEY_A = 31.5 // > 等边值 28.59（横边加长）
/// Space 半宽与列位（col 3.4324 微离半格位 5px）联合取值，同时满足：⌘–Space
/// 水平重叠与单键间一致（2(a+b)−U = 24px，横向间距均匀）+ 右缘与 N（col 4）
/// 齐平 + C/N 保持整数格心——补偿两轴步进分离（U≠V）引入的约束冲突
const SPACE_A = 137.5

/** 引出端点统一锚「远侧边中心」：平面 q=−b 边（投影为 back→right 上缘边）中点
 * (b, −KV·b/2)，沿边法向（(KU, −2)/√(4+KU²)，朝右上）外移 6px */
const CO_FAR: Pt = (() => {
  const n = vnorm([KU, -2])
  return [KEY_B + 6 * n[0], (-KV * KEY_B) / 2 + 6 * n[1]]
})()

/** 左侧边中点（平面 p=−a 边，投影 back→left 边）：(−a, −KU·a/2) 沿 −u 方向
 * 单位向量（(−2, −KU)/√(4+KU²)）外移 6px */
const CO_LEFT: Pt = (() => {
  const n = vnorm([-2, -KU])
  return [-KEY_A + 6 * n[0], (-KU * KEY_A) / 2 + 6 * n[1]]
})()

/** front→right 边（屏幕方向 (2b, −KV·b)）的朝外法向单位向量（右上向，垂直于陡升边） */
const CO_FR_NORMAL: Pt = (() => {
  const d = vnorm([2 * KEY_B, -KV * KEY_B])
  return [-d[1], d[0]]
})()

/** 右侧边中点随半宽 a 参数化：(a, KU·a/2) 沿法向外移 12px——端点正对边中心，
 * 竖直深入 16px 脱出 8px 壁带。宽板（Space）按 SPACE_A 取 */
const coRight = (a: number): Pt => [a + 12 * CO_FR_NORMAL[0], (KU * a) / 2 + 12 * CO_FR_NORMAL[1]]

// ── 引出线：端点（顶面角点）→ 斜段 → 水平段；词对齐水平段尾端（左拉 start 于左端、
// 右拉 end 于右端），居线上方 ──
interface CoSpec {
  /** 锚点角（顶面角点相对键中心的偏移） */
  corner: Pt
  /** 斜引出段向量 */
  diag: Pt
  /** 水平段长度（带方向：负左拉 / 正右拉） */
  h: number
}

function callout(cx: number, cy: number, co: CoSpec) {
  const dot: Pt = [cx + co.corner[0], cy + co.corner[1]]
  const elbow: Pt = [dot[0] + co.diag[0], dot[1] + co.diag[1]]
  const end: Pt = [elbow[0] + co.h, elbow[1]]
  // 词与水平段尾端对齐（左拉段锚线尾即左端，右拉段锚线尾即右端），基线离线 8px
  const anchor = co.h < 0 ? 'start' : 'end'
  const lx = co.h < 0 ? Math.min(elbow[0], end[0]) : Math.max(elbow[0], end[0])
  const ly = end[1] - 8
  return {
    d: `M ${pt(dot)} L ${pt(elbow)} L ${pt(end)}`,
    dot,
    lx,
    ly,
    anchor,
  }
}

/// 功能键（QWERTY 相对位置，row0 A/S/F、row1 C/N）。键名读改键 override，缺省经
/// 注册表读扩展默认（不在框架组件镜像第二份，防改键后图纸静默漂移）；板阵排布与
/// 引出线端点细节见 AGENTS.md
const FN_LAYOUT = [
  {
    extId: 'agent',
    labelKey: 'welcome.fnAgent',
    col: 0,
    row: 0,
    co: { corner: CO_FAR, diag: [-12, -28] as Pt, h: -40 },
  },
  {
    extId: 'screenshot',
    labelKey: 'welcome.fnScreenshot',
    col: 1,
    row: 0,
    co: { corner: CO_FAR, diag: [-12, -28] as Pt, h: -28 },
  },
  {
    extId: 'finder-ext',
    labelKey: 'welcome.fnFinder',
    col: 3,
    row: 0,
    co: { corner: CO_FAR, diag: [-12, -28] as Pt, h: -28 },
  },
  {
    extId: 'clipboard',
    labelKey: 'welcome.fnClipboard',
    col: 2,
    row: 1,
    // 端点在左侧边中点，同修饰键款工程折线（45° 左上斜段 + 左拉水平段），
    // 词落 A 板下方与 ⌥ 板上方的左侧空档；col 整数格心（底排整体右偏
    // U+V = 10px，排间整体平移）
    co: { corner: CO_LEFT, diag: [-22, -22] as Pt, h: -40 },
  },
  {
    extId: 'notes',
    labelKey: 'welcome.fnNotes',
    col: 4,
    row: 1,
    // 顶部远侧边锚定（CO_FAR），斜段上行右折——右邻是「/」语法键，引出线走键上方空域
    co: { corner: CO_FAR, diag: [12, -28] as Pt, h: 44 },
  },
]

/** 扩展默认键位（override 缺省时）；id 失效或未声明 default 时返回 undefined（跳过该板） */
function fnShortcut(extId: string): string | undefined {
  return getExtension(extId)?.globalShortcuts?.[0]?.default
}

/// 功能键板 + 引出线（同一过滤条件驱动，id 失效即整组跳过）。板面只标动作键——
/// 修饰语义由修饰排板与「修饰键」标注承担
const fnItems = computed(() =>
  FN_LAYOUT.flatMap((fn) => {
    const sc = settings.getShortcutOverride(fn.extId) ?? fnShortcut(fn.extId)
    if (!sc) return []
    const [cx, cy] = pos(fn.col, fn.row)
    const keys = formatShortcutKeys(sc)
    return [
      {
        id: fn.extId,
        tone: 'fn' as const,
        col: fn.col,
        row: fn.row,
        b: KEY_B,
        symbol: keys[keys.length - 1] ?? '',
        cx,
        cy,
        geom: plate(cx, cy, KEY_A, KEY_B, PLATE_T),
        labelKey: fn.labelKey,
        co: callout(cx, cy, fn.co),
      },
    ]
  }),
)

/// 主快捷键板阵：按完整 token 派生（ShortcutInput 允许 1~4 段）——修饰键依次落
/// col0/col1，空槽渲染弱化占位板；末位动作键落宽板，第 3+ 修饰无处安放并入宽板
/// 键名。主次分层：main 重笔 / ghost 弱化。派生规则详见 AGENTS.md
const mainPlates = computed(() => {
  const keys = shortcutKeys.value
  if (keys.length === 0) return []
  const mods = keys.slice(0, -1)
  const shown = mods.slice(0, 2)
  const wide = [...mods.slice(2), keys[keys.length - 1]!].join('')
  const plates = [0, 1].map((col) => {
    const sym = shown[col]
    const [cx, cy] = pos(col, 2)
    return {
      id: sym ? `mod${col}` : `ghost${col}`,
      tone: sym ? ('main' as const) : ('ghost' as const),
      col,
      row: 2,
      b: KEY_B,
      symbol: sym ?? (col === 0 ? '⌥' : '⌘'),
      cx,
      cy,
      geom: plate(cx, cy, KEY_A, KEY_B, PLATE_T),
    }
  })
  const [scx, scy] = pos(3.4324, 2)
  plates.push({
    id: 'space',
    tone: 'main' as const,
    col: 3.4324,
    row: 2,
    b: KEY_B,
    symbol: wide,
    cx: scx,
    cy: scy,
    geom: plate(scx, scy, SPACE_A, KEY_B, PLATE_T),
  })
  return plates
})

/// 修饰键引出线：锚 col0 修饰板远侧边中心，斜段左上折、水平左拉。
/// 无修饰键的组合（单键）不渲染——col0 是弱化占位非主键
const modifierCallout = computed(() => {
  if (shortcutKeys.value.length < 2) return null
  const [cx, cy] = pos(0, 2)
  return callout(cx, cy, { corner: CO_FAR, diag: [-22, -22], h: -40 })
})

/// Space（动作宽板）引出线：锚宽板右侧边中点沿边法向外移 12px（正对边中心、
/// 脱出壁带），斜段下行右折（转折开口向上），文字对齐水平段右端（右拉）
const spaceCallout = computed(() => {
  if (shortcutKeys.value.length === 0) return null
  const [cx, cy] = pos(3.4324, 2)
  return callout(cx, cy, { corner: coRight(SPACE_A), diag: [16, 16], h: 60 })
})

/// 「/」语法键：框架查询语法（/ 工具列表、// 网页搜索），非扩展快捷键——不读注册表，
/// 静态落在底排 N 右侧隔一格（真实键盘「/」即底排右段）
const SLASH: Pt = pos(6, 1)
const slashPlates = [
  {
    id: 'slash',
    tone: 'fn' as const,
    col: 6,
    row: 1,
    b: KEY_B,
    symbol: '/',
    cx: SLASH[0],
    cy: SLASH[1],
    geom: plate(SLASH[0], SLASH[1], KEY_A, KEY_B, PLATE_T),
  },
]
const slashCallout = callout(SLASH[0], SLASH[1], { corner: CO_FAR, diag: [12, -44], h: 60 })

/// 全部键帽板（painter 序：平面 row/col 升序——远者先画，近者正确覆盖远者的壁）
const allPlates = computed(() =>
  [...mainPlates.value, ...fnItems.value, ...slashPlates]
    .sort((x, y) => x.row - y.row || x.col - y.col)
    .map((k, i) => ({ ...k, i })),
)

// 图纸整体描述（屏读）：功能 + 键位组合
const stageAria = computed(() => `${t('welcome.isoAction')} ${shortcutKeys.value.join(' + ')}`)

/// 等距网格线：贴键底缘的横向线族（沿 u 轴）——每排键底面（柱体，顶面缘竖直下沉
/// PLATE_T）上下外侧缘线的延长。板 u 占位内几何断开——顶面半透明渐变、壁 hatch
/// 无底色，覆盖式隐藏会穿透显影；仅键间缝隙与外围可见
const GRID_STEP = 74 // u 步进（横排键距）；v 行层换算按 |V.x|=64（两轴步进已分离）
const GRID_U_MIN = -1
const GRID_X_MAX = 714 // 图纸右缘（视口钳制）
const GRID_MARGIN = 0.2 // 排带右界外露（格）
const gridLines = computed(() => {
  if (allPlates.value.length === 0) return []
  const drop = (p: Pt): Pt => [p[0], p[1] + PLATE_T]
  const line = (p: Pt, q: Pt) => `M ${pt(p)} L ${pt(q)}`
  // 线层 v → 断开区间（col 参数）：线与板顶面相交 ⇔ col ∈ [col_k ± a/74]（切边范围）
  const cuts = new Map<number, [number, number][]>()
  for (const k of allPlates.value) {
    const half = (k.id === 'space' ? SPACE_A : KEY_A) / GRID_STEP
    for (const side of [-1, 1]) {
      const v = k.row + (side * k.b) / -V[0]
      const key = Math.round(v * 1e3)
      const seg: [number, number] = [k.col - half, k.col + half]
      const arr = cuts.get(key)
      if (arr) arr.push(seg)
      else cuts.set(key, [seg])
    }
  }
  // 每排带右界：该层最右板切边 + 外露，行 0/1 层再钳图纸右缘；修饰排（v ≥ 1.5）
  // 维持收进 Space 右角（右段不出，避让 Space 引出线）
  const uEndOf = (list: [number, number][], v: number) => {
    const hi = Math.max(...list.map(([, h]) => h))
    if (v >= 1.5) return hi
    return Math.min(hi + GRID_MARGIN, (GRID_X_MAX - O[0] - V[0] * v) / GRID_STEP)
  }
  // 每层扫描断开区间，串出保留段
  const out: string[] = []
  for (const [vk, list] of cuts) {
    const v = vk / 1e3
    const uEnd = uEndOf(list, v)
    list.sort((a, b) => a[0] - b[0])
    let u = GRID_U_MIN
    for (const [lo, hi] of list) {
      if (lo > u) out.push(line(drop(pos(u, v)), drop(pos(Math.min(lo, uEnd), v))))
      if (hi > u) u = hi
    }
    if (u < uEnd) out.push(line(drop(pos(u, v)), drop(pos(uEnd, v))))
  }
  return out
})
</script>

<style scoped>
.welcome-view {
  position: relative;
  flex: 1 1 0%;
  min-height: 0;
  display: flex;
  flex-direction: column;
  align-items: stretch;
  justify-content: center;
}

/* 点阵底：图纸坐标网（淡点疏排，中心对称 + 边缘淡出） */
.welcome-view::before {
  content: '';
  position: absolute;
  inset: 0;
  pointer-events: none;
  background-image: radial-gradient(var(--color-divider) 1px, transparent 1.4px);
  background-size: 28px 28px;
  background-position: center center;
  -webkit-mask-image: radial-gradient(90% 90% at 50% 50%, #000 40%, transparent 100%);
  mask-image: radial-gradient(90% 90% at 50% 50%, #000 40%, transparent 100%);
}

/* ── stagger 入场：opacity + translateY，fill-mode backwards（禁 both，防合成层驻留）── */
.w-step {
  animation: w-in var(--duration-slow) var(--ease-out) backwards;
}
@keyframes w-in {
  from {
    opacity: 0;
    transform: translateY(8px);
  }
}

/* 图纸内注释元素淡入（板落定后标注入现，叙事顺序） */
.w-fade {
  animation: w-fade-in var(--duration-slow) var(--ease-out) backwards;
}
@keyframes w-fade-in {
  from {
    opacity: 0;
  }
}

/* ── 图纸（撑满整窗，viewBox 720×480 随窗口等比缩放）── */
.w-stage {
  position: relative;
  flex: 1 1 0%;
  min-height: 0;
  display: flex;
  align-items: center;
}
.w-iso {
  display: block;
  width: 100%;
  height: 100%;
}

/* 板面键名（贴面透视，居中；变换矩阵在模板按板中心生成） */
.w-cap {
  fill: var(--color-text-primary);
  font-family: var(--font-mono);
  font-size: 7px;
  font-weight: 500;
  text-anchor: middle;
  pointer-events: none;
}

/* 等距网格线（板下层，缝隙/外围可见）：沿两轴的格边界虚线，段长稀疏——
   线段横跨键间距，与两侧按键贴合。灰色坐标网（border token），与 accent 线系区分 */
.w-grid {
  fill: none;
  stroke: var(--color-border);
  stroke-width: 1;
  stroke-dasharray: 6 8;
  stroke-linecap: round;
}

.w-wall-l,
.w-wall-r {
  stroke: var(--color-accent);
  stroke-opacity: 0.5;
  stroke-width: 1.25;
  stroke-linejoin: round;
}
.w-wall-l {
  fill: url(#w-hatch-l);
}
.w-wall-r {
  fill: url(#w-hatch-r);
}
.w-top {
  fill: url(#w-top-grad);
  fill-opacity: 0.55;
  stroke: var(--color-accent);
  stroke-opacity: 0.5;
  stroke-width: 1.25;
  stroke-linejoin: round;
}
/* 深色：底色提为 accent-line（渐变浅染在深底不可见，token 深轨 22%） */
:root[data-theme='dark'] .w-top {
  fill: var(--accent-line);
}
/* 主次分层：main（主快捷键）重笔实面 / fn（功能键）常态 / ghost（⌘ 占位）弱化 */
.w-plate-main .w-top {
  fill-opacity: 1;
  stroke-opacity: 0.9;
}
.w-plate-main .w-wall-l,
.w-plate-main .w-wall-r {
  stroke-opacity: 0.85;
}
.w-plate-ghost .w-top {
  fill-opacity: 0.22;
  stroke-opacity: 0.35;
}
.w-plate-ghost .w-wall-l,
.w-plate-ghost .w-wall-r {
  stroke-opacity: 0.35;
}
.w-plate-ghost .w-cap {
  fill: var(--color-accent);
  opacity: 0.55;
}
/* 顶面高光：上缘棱线（accent 淡提） */
.w-sheen {
  fill: none;
  stroke: var(--color-accent);
  stroke-opacity: 0.22;
  stroke-width: 1;
  stroke-linecap: round;
}
.w-plate-main .w-sheen {
  stroke-opacity: 0.4;
}
.w-plate-ghost .w-sheen {
  opacity: 0;
}
:root[data-theme='dark'] .w-sheen {
  stroke-opacity: 0.35;
}
:root[data-theme='dark'] .w-plate-main .w-sheen {
  stroke-opacity: 0.5;
}

/* 图签（左下角应用名）：mono 主文本色，字号居键名与标注之上；
   宽度统一交 textLength 分配（zh 下与口号同宽），不另设 letter-spacing */
.w-title {
  fill: var(--color-text-primary);
  font-family: var(--font-mono);
  font-size: 20px;
  font-weight: 700;
}

/* 品牌口号：图签名字下方（常驻）；zh 与名称同宽（textLength），en 自然宽 */
.w-tagline {
  fill: var(--color-text-muted);
  font-family: var(--font-mono);
  font-size: 11px;
}

/* 图纸标注：端点 + 虚线引出 + 主词/副注两级 mono 小字（accent 线系） */
.w-callout circle {
  fill: var(--color-accent);
}
.w-callout path {
  fill: none;
  stroke: var(--color-accent);
  stroke-opacity: 0.7;
  stroke-width: 1;
  stroke-dasharray: 5 4;
}
.w-callout text {
  font-family: var(--font-mono);
}
.w-co-main {
  fill: var(--color-accent);
  font-size: 11px;
  letter-spacing: 0.06em;
}

/* 板入场：弹簧沉降（逐板 stagger 经内联 animationDelay） */
.w-key-in {
  animation: w-key-in var(--duration-slow) var(--ease-spring) backwards;
}
@keyframes w-key-in {
  from {
    opacity: 0;
    transform: translateY(12px);
  }
}

@media (prefers-reduced-motion: reduce) {
  .w-step,
  .w-fade,
  .w-key-in {
    animation: none;
  }
}
</style>
