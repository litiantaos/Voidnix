<template>
  <!-- 轨道：fill-active = 实底填充（进度/分隔档，见 docs/design.md 速查）。
       宽度由调用方决定；百分比/阶段文案由调用方在轨道旁自行排布（紧凑行内不回吐 label）。 -->
  <div
    class="progress-track fill-active"
    :class="size === 'sm' ? 'h-0.5' : 'h-1.5'"
    w="full"
    role="progressbar"
    aria-valuemin="0"
    aria-valuemax="100"
    :aria-valuenow="indeterminate ? undefined : percent"
  >
    <!-- 不定进度：窄条循环跑动（GPU 合成，不动 width 免重排） -->
    <div v-if="indeterminate" class="progress-fill progress-indeterminate" :class="TONE_BG[tone]" />
    <div v-else class="progress-fill" :class="TONE_BG[tone]" :style="{ width: `${percent}%` }" />
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'

/**
 * 进度/占比条（全项目唯一条状进度载体，见 docs/design.md「进度载体全景」）。
 * - value 数字 = 确定进度/占比（0-100）；null / 缺省 = 不定进度（后端无分母）
 * - tone：任务进度用 accent；系统占用等数据仪表按阈值切 success/warning/danger
 * - size：md 6px（任务条 / 主仪表）；sm 2px（进程细条）
 * 只管轨道与填充；百分比/阶段文案与取消按钮由调用方在轨道旁排布。
 */
const props = withDefaults(
  defineProps<{
    value?: number | null
    tone?: 'accent' | 'success' | 'warning' | 'danger'
    size?: 'md' | 'sm'
  }>(),
  { tone: 'accent', size: 'md' },
)

const indeterminate = computed(() => props.value == null)

const percent = computed(() => {
  const v = props.value ?? 0
  return Math.min(100, Math.max(0, Math.round(v)))
})

const TONE_BG = {
  accent: 'bg-accent',
  success: 'bg-success',
  warning: 'bg-warning',
  danger: 'bg-danger',
} as const
</script>

<style scoped>
.progress-track {
  border-radius: 9999px;
  overflow: hidden;
}
.progress-fill {
  height: 100%;
  border-radius: 9999px;
  /* 进度型过渡（慢一拍）：--duration-slow 语义即数据条填充，见 theme.css 基元 */
  transition: width var(--duration-slow) var(--ease-out);
}
/* 不定进度：fill 固定宽，transform 循环跑动（同 UpdateDialog 检查中跑条）。
   循环指示动画非一次性过渡，不适用 duration token（同 mica-fog 裸时长先例） */
.progress-indeterminate {
  width: 30%;
  animation: progress-run 1.4s linear infinite;
}
@keyframes progress-run {
  from {
    transform: translateX(-100%);
  }
  to {
    /* fill 左缘滑到轨道右端：位移 (1 + 0.3) / 0.3 − 1 = 433.33% 自身宽，完全出右界后重置 */
    transform: translateX(433.33%);
  }
}
@media (prefers-reduced-motion: reduce) {
  .progress-indeterminate {
    animation: none;
  }
}
</style>
