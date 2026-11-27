<template>
  <svg
    class="traffic-sparkline w-full block"
    :viewBox="`0 0 ${WIDTH} ${HEIGHT}`"
    preserveAspectRatio="none"
    :style="{ height: `${HEIGHT}px` }"
    aria-hidden="true"
  >
    <path v-if="areaPath" :d="areaPath" fill="currentColor" opacity="0.12" />
    <path
      :d="linePath"
      fill="none"
      stroke="currentColor"
      stroke-width="1.5"
      stroke-linecap="round"
      stroke-linejoin="round"
      vector-effect="non-scaling-stroke"
    />
  </svg>
</template>

<script setup lang="ts">
import { computed } from 'vue'

/** 固定 60 秒窗口：新样本从右端进入、旧样本左移；不足容量时左侧垫 0（真实样本始终
 *  右对齐，避免曲线随样本数被横向拉伸），冷启动的 0 线由此与曲线连续相接。 */
const CAPACITY = 60
const WIDTH = 100
const HEIGHT = 24
/** 上下留白：峰值不贴顶、谷值不压底 */
const PAD_Y = 2

const props = defineProps<{
  /** 每秒速率样本（bytes/s，最新在末尾） */
  values: number[]
}>()

/** 采样点：不足窗口时左侧垫 0——冷启动的 0 横线随真实曲线从右端生长而连续缩短，
 *  不会在第二帧整条消失；y 按窗口内峰值归一化（全 0 时落底线）。 */
const points = computed(() => {
  const list = props.values.slice(-CAPACITY)
  if (list.length < 2) return []
  const padded =
    list.length < CAPACITY ? [...Array<number>(CAPACITY - list.length).fill(0), ...list] : list
  const max = Math.max(1, ...padded)
  const plotH = HEIGHT - PAD_Y * 2
  return padded.map((value, i) => {
    const x = (i / (CAPACITY - 1)) * WIDTH
    const y = HEIGHT - PAD_Y - (value / max) * plotH
    return [x, y] as const
  })
})

/** 冷启动尚无曲线（样本不足两点）：铺一条 0 横线占位，避免面板留白 */
const linePath = computed(() => {
  if (points.value.length === 0) return `M0,${HEIGHT - PAD_Y} L${WIDTH},${HEIGHT - PAD_Y}`
  return points.value
    .map(([x, y], i) => `${i === 0 ? 'M' : 'L'}${x.toFixed(2)},${y.toFixed(2)}`)
    .join(' ')
})

/** 曲线下方淡填充闭合到底边：强化趋势可读性，不引入独立图例/坐标轴 */
const areaPath = computed(() => {
  const pts = points.value
  if (pts.length === 0) return ''
  const [x0] = pts[0]!
  const [xLast] = pts[pts.length - 1]!
  return `${linePath.value} L${xLast.toFixed(2)},${HEIGHT} L${x0.toFixed(2)},${HEIGHT} Z`
})
</script>
