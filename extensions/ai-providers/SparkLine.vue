<template>
  <svg
    v-if="points.length > 1"
    :width="width"
    :height="height"
    :viewBox="`0 0 ${width} ${height}`"
    class="shrink-0 block"
    aria-hidden="true"
  >
    <!-- 裸 hex 不走 CSS 变量：WKWebView 下 SVG stroke/stop-color 的 var() 常失效；
         值 = theme.css --color-accent / uno accent（#3d82f0），改 accent 色时同步 -->
    <defs>
      <linearGradient :id="gradId" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0%" :stop-color="ACCENT" stop-opacity="0.22" />
        <stop offset="100%" :stop-color="ACCENT" stop-opacity="0" />
      </linearGradient>
    </defs>
    <path :d="areaD" :fill="`url(#${gradId})`" />
    <path
      :d="lineD"
      fill="none"
      :stroke="ACCENT"
      stroke-width="1.5"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
  </svg>
</template>

<script setup lang="ts">
import { computed } from 'vue'

/// accent 色（= theme.css --color-accent / uno accent）。SVG 在 WKWebView 下 var()
/// 不可靠，故内联字面值；改 accent 色时同步 theme.css + uno.config.ts + 此常量。
const ACCENT = '#3d82f0'

const props = withDefaults(
  defineProps<{
    data: number[]
    width?: number
    height?: number
    /** 多序列共用峰值；不传则用本序列 max */
    max?: number
  }>(),
  { width: 64, height: 28 },
)

const gradId = `sp-${Math.random().toString(36).slice(2, 8)}`

const points = computed(() => {
  const raw = props.data.filter((n) => Number.isFinite(n))
  if (raw.length < 2) return [] as { x: number; y: number }[]
  const w = props.width
  const h = props.height
  const padY = 3
  const peak = Math.max(props.max && props.max > 0 ? props.max : 0, ...raw, 1)
  const n = raw.length
  return raw.map((v, i) => ({
    x: n === 1 ? w / 2 : (i / (n - 1)) * w,
    y: h - padY - (v / peak) * (h - padY * 2),
  }))
})

const lineD = computed(() => {
  const pts = points.value
  if (pts.length < 2) return ''
  let d = `M${pts[0]!.x.toFixed(2)},${pts[0]!.y.toFixed(2)}`
  for (let i = 1; i < pts.length; i++) {
    const a = pts[i - 1]!
    const b = pts[i]!
    const cx = (a.x + b.x) / 2
    d += ` C${cx.toFixed(2)},${a.y.toFixed(2)} ${cx.toFixed(2)},${b.y.toFixed(2)} ${b.x.toFixed(2)},${b.y.toFixed(2)}`
  }
  return d
})

const areaD = computed(() => {
  const d = lineD.value
  if (!d) return ''
  const w = props.width
  const h = props.height
  return `${d} L${w},${h} L0,${h} Z`
})
</script>
