<template>
  <div class="welcome-view" :class="{ 'w-expanded': expanded }">
    <!-- 主体：键盘键位图（等距图纸）——纯信息展示零交互，描述全部经引出线标注；
         展开权限时几何内容（板阵/网格/标注）整体等比缩小，图签不随移动原地渐隐 -->
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

        <!-- 缩放层：展开时几何内容整体等比缩小（viewBox 中心原地收缩）；图签在层外，
             不随移动——展开时原地渐隐 -->
        <g class="w-zoom" :transform="stageZoom">
          <!-- 等距网格线（板下层，穿过板面的部分被板覆盖）——全部段合并单 path
              （dash 每子路径重启，与分段等价；单元素减每帧 attribute 写入与失效开销） -->
          <path class="w-grid" :d="gridPath" />

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
            <text class="w-cap" :transform="capXform(k.cx, k.cy)" x="0" y="1.5">
              {{ k.symbol }}
            </text>
          </g>

          <!-- 修饰键引出线：端点在 col0 修饰板远侧边中心，文字对齐水平段左端（左拉）。
             与其余引出线同经 callout() 计算；单键组合无修饰键不渲染 -->
          <g v-if="modifierCallout" class="w-callout w-fade" :style="{ animationDelay: '420ms' }">
            <circle :cx="modifierCallout.dot[0]" :cy="modifierCallout.dot[1]" r="3" />
            <path :d="modifierCallout.d" />
            <!-- 标注词定位走 transform（x/y attribute 变化会触发 SVG text 逐帧重排，
               transform 只重绘不重排——动画帧预算关键） -->
            <text
              class="w-co-main"
              :transform="coText(modifierCallout.lx, modifierCallout.ly)"
              :text-anchor="modifierCallout.anchor"
            >
              {{ t('welcome.isoModifier') }}
            </text>
          </g>

          <!-- 功能键引出线：全部经线拉出（端点 + 虚线 + 词） -->
          <g
            v-for="fn in fnCallouts"
            :key="fn.id + '-co'"
            class="w-callout w-fade"
            :style="{ animationDelay: `${470 + fn.fi * 60}ms` }"
          >
            <circle :cx="fn.co.dot[0]" :cy="fn.co.dot[1]" r="3" />
            <path :d="fn.co.d" />
            <text
              class="w-co-main"
              :transform="coText(fn.co.lx, fn.co.ly)"
              :text-anchor="fn.co.anchor"
            >
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
              :transform="coText(spaceCallout.lx, spaceCallout.ly)"
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
              :transform="coText(slashCallout.lx, slashCallout.ly - 13)"
              :text-anchor="slashCallout.anchor"
            >
              {{ t('welcome.noteTools') }}
            </text>
            <text
              class="w-co-main"
              :transform="coText(slashCallout.lx, slashCallout.ly)"
              :text-anchor="slashCallout.anchor"
            >
              {{ t('welcome.noteSearch') }}
            </text>
          </g>
        </g>
        <!-- 图签：左下角应用名（入场序列末位，随标注之后淡入）+ 品牌口号（名字下方，
             底缘沉板阵最低点下方）。同宽约束仅 zh：CJK 等宽字形无感；en 词形长，
             压同宽会挤字距，保持自然宽左对齐 -->
        <text
          class="w-title w-fade"
          x="53"
          y="435"
          textLength="96"
          :style="{ animationDelay: '660ms' }"
        >
          Voidnix
        </text>
        <text
          class="w-tagline w-fade"
          x="53"
          y="455"
          :textLength="taglineLength"
          :style="{ animationDelay: '660ms' }"
        >
          {{ t('welcome.tagline') }}
        </text>
      </svg>
    </div>

    <!-- 底部操作提示（右下角常驻单句，同一元素同一位置，与左下角图签构成 L 形
         双锚点）：内容随状态切换——
           图纸态 Enter / → 下一步 · ← 上一步；展开态 ← 上一步 · Enter 开始使用。
           切换经 keyed Transition 错峰交叉淡化：旧句快出、新句慢浮（浮现过程
           可感知），全程无空白低可见段（不闪）零位移；离场元素脱流铺同一
           位置，流内恒一元素不跳 -->
    <div class="w-footer w-fade" :style="{ animationDelay: '780ms' }">
      <Transition name="w-swap">
        <div class="w-note" :key="expanded ? 'done' : 'next'">
          {{ t(expanded ? 'welcome.navDone' : 'welcome.navHint') }}
        </div>
      </Transition>
    </div>

    <!-- 下方权限面板（展开态滑入）：功能 ↔ 权限映射 + 授权入口，条目以细分隔线分组；
         图纸同步转变为俯视平面图并等比缩小让位，面板上移与底部提示留距。
         收起态 inert——按钮不可聚焦，防授权后收起的残留焦点吞掉 Enter（误重开系统设置） -->
    <aside class="w-perm-panel soft-card" :aria-hidden="!expanded" :inert="!expanded">
      <div class="w-perm-list">
        <div v-for="p in permRows" :key="p.kind" class="w-perm-row">
          <div class="w-perm-name">{{ p.label }}</div>
          <div class="w-perm-use">{{ p.use }}</div>
          <BaseButton
            v-if="p.granted !== true"
            class="w-perm-btn"
            icon="i-ri-alert-line"
            @click="handlePerm(p.kind)"
          >
            {{ t('welcome.permGrant') }}
          </BaseButton>
          <div v-else class="w-perm-done">
            <i class="i-ri-checkbox-circle-line" />
            <span>{{ t('welcome.permGranted') }}</span>
          </div>
        </div>
      </div>
    </aside>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { onKeyStroke } from '@/composables/events'
import { useSettingsStore } from '@/stores/settings'
import { useAppStore } from '@/stores/app'
import { useSystemStore } from '@/stores/system'
import { CMD } from '@/commands'
import { isTauri } from '@/utils/tauri'
import BaseButton from '@/components/ui/BaseButton.vue'
import { formatShortcutKeys } from '@/utils/format'
import { getExtension } from '@/runtime/extension-registry'
import { t, locale } from '@/runtime/i18n'

/// 首启引导（fullscreen 槽供给方，MainView 整窗层渲染）：两页——页 0 键位图、
/// 页 1 权限说明。槽契约：Enter 前进/末页完结、Esc 随时完结——自持落盘 onboarded
/// 后 emit done 仅请求框架收尾（框架键盘已让位，组件自治按键）。
const emit = defineEmits<{ done: [] }>()

const settings = useSettingsStore()
const appStore = useAppStore()
const systemStore = useSystemStore()

type PermKind = 'screen_recording' | 'accessibility' | 'full_disk_access'

/// 引导单界面交互排版：expanded=false 图纸等距撑满；Enter / → 展开权限——图纸经
/// 投影因子 f 连续转变为俯视平面图（见几何段）、整体等比缩小（isoStyle）并上移，
/// 下方滑入权限面板；展开态 Enter 完结、Esc 随时完结、← 收起。f 由 rAF 驱动
///（SVG attribute 几何无法 CSS 过渡，属「布局伸缩用 JS hooks」合规场景）
const expanded = ref(false)
const expandF = ref(0)
let expandRaf = 0

function setExpanded(to: boolean) {
  if (expanded.value === to) return
  expanded.value = to
  cancelAnimationFrame(expandRaf)
  if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
    expandF.value = to ? 1 : 0
    return
  }
  const from = expandF.value
  const start = performance.now()
  const step = (now: number) => {
    const t = Math.min(1, (now - start) / 300)
    expandF.value = from + ((to ? 1 : 0) - from) * (1 - Math.pow(1 - t, 3))
    if (t < 1) expandRaf = requestAnimationFrame(step)
  }
  expandRaf = requestAnimationFrame(step)
}

onUnmounted(() => cancelAnimationFrame(expandRaf))

/// 展开态图纸整体等比缩小（为权限面板让位）：几何内容经根 g attribute transform
/// 缩放，scale 随 expandF 逐帧插值、与投影几何同帧同步（attribute 几何无法 CSS
/// 过渡，同「布局伸缩用 JS hooks」合规场景）。attribute scale 围绕 viewBox 原点，
/// 平移补偿到中心 (360, 240)（meet 居中即视觉中心）实现原地收缩；图签在缩放层外
const TOP_SCALE = 0.8 // 俯视终点整体缩放（等距 1）
const stageZoom = computed(() => {
  const s = 1 - (1 - TOP_SCALE) * expandF.value
  return `translate(${(360 * (1 - s)).toFixed(2)} ${(240 * (1 - s)).toFixed(2)}) scale(${s.toFixed(4)})`
})

/// 权限页条目（功能 ↔ 权限映射）：granted null（检查中）按未授权渲染，
/// 状态由获焦刷新链路更新
const permRows = computed(
  () =>
    [
      {
        kind: 'screen_recording',
        label: t('welcome.permScreenRecording'),
        use: t('welcome.permUseScreenRecording'),
        granted: systemStore.permScreenRecording,
      },
      {
        kind: 'accessibility',
        label: t('welcome.permAccessibility'),
        use: t('welcome.permUseAccessibility'),
        granted: systemStore.permAccessibility,
      },
      {
        kind: 'full_disk_access',
        label: t('welcome.permFullDisk'),
        use: t('welcome.permUseFullDisk'),
        granted: systemStore.permFullDiskAccess,
      },
    ] as { kind: PermKind; label: string; use: string; granted: boolean | null }[],
)

/// 未授权项点击直达系统设置（同设置页权限行范式：辅助功能先经系统弹窗请求再跳）；
/// 授权后返回必经窗口获焦，状态自动刷新
async function handlePerm(kind: PermKind) {
  if (!isTauri) return
  if (kind === 'accessibility') {
    systemStore.permAccessibility = await invoke<boolean>(CMD.requestAccessibilityPermission)
  }
  await invoke(CMD.openPrivacySettings, { kind })
}

const shortcutKeys = computed(() => formatShortcutKeys(settings.globalShortcut))

/// 口号同宽约束仅 zh（CJK 等宽字形分配无感）；en 词形长，压到 96 会挤字距
const taglineLength = computed(() => (locale.value === 'zh-CN' ? 96 : undefined))

onKeyStroke(['Enter', 'Escape', 'ArrowLeft', 'ArrowRight'], (e) => {
  // Enter 焦点在按钮（权限授权等）上时让位给按钮自身激活（BaseList 同款守卫），防双发
  if (e.key === 'Enter' && document.activeElement?.tagName === 'BUTTON') return
  if (e.key === 'Escape') {
    requestDone()
  } else if (e.key === 'ArrowRight') {
    setExpanded(true)
  } else if (e.key === 'ArrowLeft') {
    setExpanded(false)
  } else if (!expanded.value) {
    setExpanded(true)
  } else {
    requestDone()
  }
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
const lerp = (x: number, y: number, t: number): number => x + (y - x) * t

/// 投影插值（等距 → 俯视）：f=0 等距；f=1 纯俯视正交平面图。**俯视终点 = 等距
/// 投影的视觉尺寸**（同一把键盘，仅视角转正）——键横宽 = 2a·u₁ = 等距 u 边视觉长
/// 65.8、键纵高 = 2b·kv = 等距 v 边视觉长 59.7、排距 = 等距 |V| 108.9、行错位 =
/// 等距屏幕错位 +10px、字符视觉轴长不变。转变中变化的只有投影形状（菱形→矩形）、
/// 侧壁（厚度收敛 0）、col 向 y 下沉（排转平）与原点平移（俯视内容在面板上方区域
/// 居中——x 向板阵与右侧词块对称配重、y 向可见区中心）
const ISO = { u1: 1, ku: KU / 2, kx: -1, kxr: -1, kv: KV / 2, kvr: KV / 2, ox: 230, oy: 119 }
/// VIS = 各轴等距视觉缩放因子（√(4+K²)/2）：u 边 1.0444 / v 边 1.7023
const VIS = Math.sqrt(4 + KV * KV) / 2
const TOP = {
  u1: Math.sqrt(4 + KU * KU) / 2,
  ku: 0,
  kx: 0,
  kxr: 10 / 64,
  kv: VIS,
  kvr: VIS,
  ox: 111,
  oy: 104,
}

const ax = computed(() => {
  const f = expandF.value
  return {
    u1: lerp(ISO.u1, TOP.u1, f),
    ku: lerp(ISO.ku, TOP.ku, f),
    kx: lerp(ISO.kx, TOP.kx, f),
    kxr: lerp(ISO.kxr, TOP.kxr, f),
    kv: lerp(ISO.kv, TOP.kv, f),
    kvr: lerp(ISO.kvr, TOP.kvr, f),
    ox: lerp(ISO.ox, TOP.ox, f),
    oy: lerp(ISO.oy, TOP.oy, f),
    t: 8 * (1 - f), // 板厚（等距 8 → 俯视 0）
  }
})

/// 键名贴面矩阵：文字基向量 = 贴面轴（等距 x̂=(CAP_STRETCH, CAP_STRETCH·ku)、
/// ŷ=(−1,kv)）。u/v 轴投影长度不对称会把字面横向压扁，x 列按两轴长度比拉伸补偿
///（CAP_STRETCH）——补偿后两轴视觉长度相等（√(1+kv²)），俯视保持同一视觉轴长
///（x̂=(VIS,0)、ŷ=(0,VIS)），字符视觉大小全程不变
const CAP_STRETCH = Math.sqrt(4 + KV * KV) / Math.sqrt(4 + KU * KU)
const capXform = (cx: number, cy: number) => {
  const A = ax.value
  const sa = lerp(CAP_STRETCH, VIS, expandF.value)
  return `matrix(${sa.toFixed(4)} ${(sa * A.ku).toFixed(4)} ${A.kx.toFixed(4)} ${lerp(A.kv, VIS, expandF.value).toFixed(4)} ${cx} ${cy})`
}

function isoCorners(cx: number, cy: number, a: number, b: number): Pt[] {
  // 返回 [前(下), 右, 后(上), 左] 四角：角 = 中心 + p·Û + q·V̂（p=±a、q=±b）
  const A = ax.value
  const yF = A.ku * a + A.kv * b
  const yR = A.ku * a - A.kv * b
  const fx = (p: number, q: number) => cx + p * A.u1 + q * A.kx
  return [
    [fx(a, b), cy + yF],
    [fx(a, -b), cy + yR],
    [fx(-a, -b), cy - yF],
    [fx(-a, b), cy - yR],
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

// ── 键盘网格：pos(col, row) = 原点 + col·U + row·Vrow（U 沿排 / Vrow 沿行）──
// 等距平面网格矩形化：横排键距 74、排距 64（≈0.86 键距同真实键盘），排间整体
// 平移（C 对 S、N 对 F 偏移 U+Vrow = 10px）。轴随投影插值（ax）——俯视保留行错位
const pos = (col: number, row: number): Pt => {
  const A = ax.value
  return [A.ox + col * 74 * A.u1 + row * 64 * A.kxr, A.oy + col * 74 * A.ku + row * 64 * A.kvr]
}

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
 * −b·V̂，沿边法向（Û 旋转 −90°，等距朝右上、俯视正上）外移 6px。随投影插值 */
const CO_FAR = computed<Pt>(() => {
  const A = ax.value
  const m: Pt = [-KEY_B * A.kx, -KEY_B * A.kv]
  const n = vnorm([A.ku, -A.u1])
  return [m[0] + 6 * n[0], m[1] + 6 * n[1]]
})

/** 左侧边中点（平面 p=−a 边，投影 back→left 边）：−a·Û 沿边法向外移 6px */
const CO_LEFT = computed<Pt>(() => {
  const A = ax.value
  const n = vnorm([-A.u1, -A.ku])
  return [-KEY_A * A.u1 + 6 * n[0], -KEY_A * A.ku + 6 * n[1]]
})

/** 右侧边中点随半宽 a 参数化：a·Û 沿边法向（−V̂ 旋转 +90°，等距右上、俯视正右）
 * 外移 12px——端点正对边中心。宽板（Space）按 SPACE_A 取 */
const coRight = (a: number): Pt => {
  const A = ax.value
  const n = vnorm([A.kv, -A.kx])
  return [a * A.u1 + 12 * n[0], a * A.ku + 12 * n[1]]
}

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
/// 引出线端点细节见 AGENTS.md。corner 用锚标记（far/left），实际向量随投影插值
const FN_LAYOUT = [
  {
    extId: 'agent',
    labelKey: 'welcome.fnAgent',
    col: 0,
    row: 0,
    co: { corner: 'far' as const, diag: [-12, -28] as Pt, h: -40 },
  },
  {
    extId: 'screenshot',
    labelKey: 'welcome.fnScreenshot',
    col: 1,
    row: 0,
    co: { corner: 'far' as const, diag: [-12, -28] as Pt, h: -28 },
  },
  {
    extId: 'finder-ext',
    labelKey: 'welcome.fnFinder',
    col: 3,
    row: 0,
    co: { corner: 'far' as const, diag: [-12, -28] as Pt, h: -28 },
  },
  {
    extId: 'clipboard',
    labelKey: 'welcome.fnClipboard',
    col: 2,
    row: 1,
    // 端点在左侧边中点，同修饰键款工程折线（45° 左上斜段 + 左拉水平段），
    // 词落 A 板下方与 ⌥ 板上方的左侧空档；col 整数格心（底排整体右偏
    // U+V = 10px，排间整体平移）
    co: { corner: 'left' as const, diag: [-22, -22] as Pt, h: -40 },
  },
  {
    extId: 'notes',
    labelKey: 'welcome.fnNotes',
    col: 4,
    row: 1,
    // 顶部远侧边锚定（far），斜段上行右折——右邻是「/」语法键，引出线走键上方空域
    co: { corner: 'far' as const, diag: [12, -28] as Pt, h: 44 },
  },
]

/// 引出锚点表（标记 → 当前投影下的向量）
const anchors = computed<Record<'far' | 'left', Pt>>(() => ({
  far: CO_FAR.value,
  left: CO_LEFT.value,
}))

/** 扩展默认键位（override 缺省时）；id 失效或未声明 default 时返回 undefined（跳过该板） */
function fnShortcut(extId: string): string | undefined {
  return getExtension(extId)?.globalShortcuts?.[0]?.default
}

/// 功能键板静态描述（同一过滤条件驱动板与引出线，id 失效即整组跳过）。板面只标
/// 动作键——修饰语义由修饰排板与「修饰键」标注承担
const fnSpecs = computed(() =>
  FN_LAYOUT.flatMap((fn) => {
    const sc = settings.getShortcutOverride(fn.extId) ?? fnShortcut(fn.extId)
    if (!sc) return []
    const keys = formatShortcutKeys(sc)
    return [
      {
        id: fn.extId,
        col: fn.col,
        row: fn.row,
        symbol: keys[keys.length - 1] ?? '',
        labelKey: fn.labelKey,
        co: fn.co,
      },
    ]
  }),
)

/// 主快捷键板阵静态描述：按完整 token 派生（ShortcutInput 允许 1~4 段）——修饰键
/// 依次落 col0/col1，空槽渲染弱化占位板；末位动作键落宽板，第 3+ 修饰无处安放
/// 并入宽板键名。主次分层：main 重笔 / ghost 弱化。派生规则详见 AGENTS.md
const mainSpecs = computed(() => {
  const keys = shortcutKeys.value
  if (keys.length === 0) return []
  const mods = keys.slice(0, -1)
  const shown = mods.slice(0, 2)
  const wide = [...mods.slice(2), keys[keys.length - 1]!].join('')
  const specs = [0, 1].map((col) => {
    const sym = shown[col]
    return {
      id: sym ? `mod${col}` : `ghost${col}`,
      tone: (sym ? 'main' : 'ghost') as 'main' | 'ghost',
      col,
      row: 2,
      a: KEY_A,
      b: KEY_B,
      symbol: sym ?? (col === 0 ? '⌥' : '⌘'),
    }
  })
  specs.push({
    id: 'space',
    tone: 'main' as const,
    col: 3.4324,
    row: 2,
    a: SPACE_A,
    b: KEY_B,
    symbol: wide,
  })
  return specs
})

/// 「/」语法键：框架查询语法（/ 工具列表、// 网页搜索），非扩展快捷键——不读注册表，
/// 静态落在底排 N 右侧隔一格（真实键盘「/」即底排右段）
const SLASH_SPEC = {
  id: 'slash',
  tone: 'fn' as const,
  col: 6,
  row: 1,
  a: KEY_A,
  b: KEY_B,
  symbol: '/',
}

/// 板静态规格（painter 序：平面 row/col 升序——远者先画，近者正确覆盖远者的壁；
/// i 为入场 stagger 序）。静态层仅在改键/注册表变化时重算；投影插值动画每帧只
/// 重算几何（allPlates），静态/动态拆分削减逐帧分配与字符串开销
const plateSpecs = computed(() =>
  [
    ...mainSpecs.value,
    ...fnSpecs.value.map(({ id, col, row, symbol }) => ({
      id,
      tone: 'fn' as const,
      col,
      row,
      a: KEY_A,
      b: KEY_B,
      symbol,
    })),
    SLASH_SPEC,
  ]
    .sort((x, y) => x.row - y.row || x.col - y.col)
    .map((k, i) => ({ ...k, i })),
)

/// 全部键帽板几何（每帧随投影插值重算：板心 + 顶面/左右壁/高光四面）
const allPlates = computed(() =>
  plateSpecs.value.map((s) => {
    const [cx, cy] = pos(s.col, s.row)
    return { ...s, cx, cy, geom: plate(cx, cy, s.a, s.b, ax.value.t) }
  }),
)

/// 功能键引出线（几何每帧随投影插值；词与 stagger 序自静态 fnSpecs 派生）
const fnCallouts = computed(() =>
  fnSpecs.value.map((fn, fi) => {
    const [cx, cy] = pos(fn.col, fn.row)
    return {
      id: fn.id,
      labelKey: fn.labelKey,
      fi,
      co: callout(cx, cy, { ...fn.co, corner: anchors.value[fn.co.corner] }),
    }
  }),
)

/// 标注词定位：transform 而非 x/y attribute——transform 更新只重绘不触发 SVG text
/// 逐帧重排（x/y 是布局输入），动画帧预算关键
const coText = (x: number, y: number) => `translate(${x.toFixed(2)} ${y.toFixed(2)})`

/// 修饰键引出线：锚 col0 修饰板远侧边中心，斜段左上折、水平左拉。
/// 无修饰键的组合（单键）不渲染——col0 是弱化占位非主键
const modifierCallout = computed(() => {
  if (shortcutKeys.value.length < 2) return null
  const [cx, cy] = pos(0, 2)
  return callout(cx, cy, { corner: CO_FAR.value, diag: [-22, -22], h: -40 })
})

/// Space（动作宽板）引出线：锚宽板右侧边中点沿边法向外移 12px（正对边中心）。
/// 斜段随投影插值——等距下行右折（转折开口向上，避开板阵），俯视上行右折
///（下缘已收敛、下行会落进面板区）；h 随俯视端点右移（dot +49px）对冲收窄，
/// 词块尾端不随板远漂
const spaceCallout = computed(() => {
  if (shortcutKeys.value.length === 0) return null
  const [cx, cy] = pos(3.4324, 2)
  const f = expandF.value
  const diag: Pt = [lerp(16, 10, f), lerp(16, -40, f)]
  return callout(cx, cy, { corner: coRight(SPACE_A), diag, h: lerp(60, 52, f) })
})

/// 「/」语法键引出线（几何随投影插值 → computed）
const slashCallout = computed(() => {
  const [sx, sy] = pos(6, 1)
  // h 随投影收窄：等距端点 60 保持静息态不变（尾端 701 在图纸右缘 714 内）；
  // 俯视板随原点左移 −25px，收至 25 使词块右缘贴键右缘
  return callout(sx, sy, { corner: CO_FAR.value, diag: [12, -44], h: lerp(60, 25, expandF.value) })
})

// 图纸整体描述（屏读）：功能 + 键位组合
const stageAria = computed(() => `${t('welcome.isoAction')} ${shortcutKeys.value.join(' + ')}`)

/// 网格线：贴键底缘的横向线族（沿 u 轴）——每排键底面（柱体，顶面缘竖直下沉 t）
/// 上下外侧缘线的延长。板 u 占位内几何断开——顶面半透明渐变、壁 hatch 无底色，
/// 覆盖式隐藏会穿透显影；仅键间缝隙与外围可见。轴/壁厚随投影插值（俯视 t→0 贴顶面）。
/// 全部段合并为单 path（dash 每子路径重启，与分段 path 渲染等价）
const GRID_STEP = 74 // u 平面步进（横排键距）；v 行层换算按 |V.x 平面|=64
const GRID_U_MIN = -1
const GRID_X_MAX = 714 // 图纸右缘（视口钳制）
const GRID_MARGIN = 0.2 // 排带右界外露（格）
const gridPath = computed(() => {
  const specs = plateSpecs.value
  if (specs.length === 0) return ''
  const A = ax.value
  const drop = (p: Pt): Pt => [p[0], p[1] + A.t]
  const line = (p: Pt, q: Pt) => `M ${pt(p)} L ${pt(q)}`
  // 线层 v → 断开区间（col 参数）：线与板顶面相交 ⇔ col ∈ [col_k ± a·u1/74]（切边范围）
  const cuts = new Map<number, [number, number][]>()
  for (const k of specs) {
    const half = k.a / GRID_STEP
    for (const side of [-1, 1]) {
      // 线层 v → 断开区间（col 参数）：线与板顶面相交 ⇔ col ∈ [col_k ± a·u1/74]；
      // 层的 v 偏移按键高/行距比（b·kv/(64·kvr)），行错位率 kxr 只影响 x 投影
      const v = k.row + (side * k.b * A.kv) / -(64 * A.kvr)
      const key = Math.round(v * 1e3)
      const seg: [number, number] = [k.col - half * A.u1, k.col + half * A.u1]
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
    return Math.min(hi + GRID_MARGIN, (GRID_X_MAX - A.ox - 64 * A.kxr * v) / (74 * A.u1))
  }
  // 每层扫描断开区间，串出保留段；起点按层钳 x ≥ 6（俯视行偏移下 u=−1 可能出左界）
  const uStart = (v: number) => Math.max(GRID_U_MIN, (6 - A.ox - 64 * A.kxr * v) / (74 * A.u1))
  const out: string[] = []
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

/* ── 图纸（撑满整窗，viewBox 720×480 随窗口等比缩放；展开态 transform 见交互排版段）── */
.w-stage {
  position: relative;
  flex: 1 1 0%;
  min-height: 0;
  display: flex;
  align-items: center;
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

/* 底部操作提示（右下角常驻，与左下角图签 L 形配重）：内容随状态切换（navHint /
   navDone）。min-height = 行高 12（+ 底距 8 = 总高 20，与无提示态一致）——离场
   元素脱流、流内恒一元素，交叉淡化全程高度恒定，不牵动上方图纸区域 */
.w-footer {
  position: relative; /* 离场元素 absolute 铺位的定位上下文 */
  display: flex;
  justify-content: flex-end;
  min-height: 12px;
  padding: 0 12px 8px;
}
/* mono 小字 muted；行高显式 12 与 footer 定高（12 + 8）精确呼应 */
.w-note {
  color: var(--color-text-muted);
  font-family: var(--font-mono);
  font-size: 10px;
  line-height: 12px;
  letter-spacing: 0.04em;
}

/* 提示切换：错峰交叉淡化——旧句 100ms 快出、新句 300ms 慢浮（浮现过程可感知）。
   两句可见度之和全程 ≥ ~0.6，无空白低可见段（out-in 先出后进会闪）；零位移纯
   opacity（任何 transform 位移都会被感知为位置变化）。离场元素 absolute 脱流
   并铺回同一位置（同对齐同行高）——流内恒一元素，右对齐与高度全程恒定 */
.w-swap-enter-active {
  transition: opacity var(--duration-slow) var(--ease-out);
}
.w-swap-leave-active {
  transition: opacity var(--duration-fastest) var(--ease-in);
  position: absolute;
  inset: 0 12px;
  display: flex;
  justify-content: flex-end;
  align-items: flex-start;
}
.w-swap-enter-from,
.w-swap-leave-to {
  opacity: 0;
}

/* ── 展开权限交互排版：图纸经投影因子转变为俯视平面图（script rAF 驱动几何），
   几何内容经根 g attribute transform 整体等比缩小（随 expandF 插值 1→0.8，
   见 script；图签在缩放层外原地渐隐）；下方面板滑入 ── */
.w-iso {
  display: block;
  width: 100%;
  height: 100%;
}
/* 图签与口号展开态淡出（面板覆盖其区域，收起即回归）。淡出走 fastest+ease-out
   前置快消——先于面板滑入清空其落位区域防重叠；收起反向（面板快出、标题
   ease-in 慢起）天然错峰 */
.w-title,
.w-tagline {
  transition: opacity var(--duration-fast) var(--ease-in);
}
.w-expanded .w-title,
.w-expanded .w-tagline {
  opacity: 0;
  transition: opacity var(--duration-fastest) var(--ease-out);
}

.w-perm-panel {
  position: absolute;
  /* soft-card 抬升卡框（可见边界 + 磨砂面）——无边框时三列左对齐文字块
     硬贴隐形左缘、右侧留空，观感偏左；框立则居中立现 */
  padding: 12px;
  /* 居中限宽：与图纸（SVG meet 居中）同轴对齐，不贴窗口边；
     bottom 40——与底部常驻提示留距，卡片顶缘不贴俯视图 */
  left: 12px;
  right: 12px;
  bottom: 40px;
  max-width: 640px;
  margin: 0 auto;
  opacity: 0;
  transform: translateY(16px);
  transition:
    opacity var(--duration-normal) var(--ease-out),
    transform var(--duration-normal) var(--ease-out);
  pointer-events: none;
}
.w-expanded .w-perm-panel {
  opacity: 1;
  transform: none;
  pointer-events: auto;
}

.w-perm-list {
  display: flex;
  gap: 12px;
}
/* 三列横排（压缩面板高度，给俯视图留出完整空间）；列间细分隔线 */
.w-perm-row {
  flex: 1 1 0%;
  min-width: 0;
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 6px;
}
.w-perm-row + .w-perm-row {
  border-left: 1px solid var(--color-divider);
  padding-left: 12px;
}
.w-perm-name {
  color: var(--color-text-primary);
  font-size: 13px;
  font-weight: 500;
}
.w-perm-use {
  color: var(--color-text-muted);
  font-size: 12px;
}
.w-perm-btn {
  font-size: 12px;
  padding: 1px 10px;
}
/* 已授权完成态：success 语义绿（勾 + 文字同色） */
.w-perm-done {
  display: flex;
  align-items: center;
  gap: 6px;
  color: var(--color-success);
  font-size: 12px;
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
  .w-perm-panel,
  .w-title,
  .w-tagline {
    transition: none;
  }
  .w-swap-enter-active,
  .w-swap-leave-active {
    transition: none;
  }
}
</style>
