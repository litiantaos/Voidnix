<template>
  <button
    type="button"
    :disabled="disabled || loading"
    :aria-busy="loading || undefined"
    :title="title"
    :aria-label="isIconOnly() && title ? title : undefined"
    :class="[
      'ui-ctrl flex-none',
      isIconOnly() ? 'w-7 px-0 flex-center' : icon || loading ? 'flex gap-2 items-center' : '',
      size === 'inline' ? 'ui-btn-inline' : '',
      surfaceClass,
      variantClass,
      disabled || loading ? 'ui-disabled' : '',
      loading ? 'ui-loading' : '',
      active ? 'ui-active' : '',
    ]"
  >
    <i v-if="loading" class="i-ri-loader-4-line text-sm animate-spin" />
    <i v-else-if="icon" :class="[icon, 'text-sm']" />
    <slot />
  </button>
</template>

<script setup lang="ts">
import { computed, useSlots } from 'vue'

interface Props {
  variant?: 'default' | 'primary' | 'ghost' | 'danger'
  disabled?: boolean
  icon?: string
  /** hover 提示；icon-only 时自动透传为 aria-label（无障碍命名统一由组件收口） */
  title?: string
  /** 显式 active 状态（列表/弹窗焦点环视觉提示，避免作为 fallthrough attr 污染 DOM） */
  active?: boolean
  /** 尺寸变体：default = ui-ctrl 壳（h-7/px-3）；inline = 表单内联图标钮（输入框行内
   *  眼睛钮等，随文字行高、贴边内距、muted 色）。! 前缀是必须的：覆盖 ui-ctrl 壳的
   *  同属性类（h-7/px-3/text-primary），裸类并存时胜负取决于 UnoCSS 生成顺序 */
  size?: 'default' | 'inline'
  /** 加载态：前置 spinner（i-ri-loader-4-line animate-spin，text-sm 与 icon 同大，
   *  loading ↔ idle 切换零尺寸跳变）+
   * 禁交互（同 disabled），主钮面色降级（范式见 docs/design.md ui-btn-*） */
  loading?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  variant: 'default',
  disabled: false,
  active: false,
  loading: false,
  size: 'default',
})

const slots = useSlots()

/** icon-only 判定在 render 期求值：slots.default 存在性不参与响应式（动态 slot
 *  集合由父 render 驱动同步），computed 缓存会在 slot 缺席后依赖集为空、锁死旧值 */
function isIconOnly(): boolean {
  return !!props.icon && !slots.default
}

/**
 * 面选择：
 * - active + default：保留 soft-chip（1px 边以透明占位由 .soft-chip.ui-active 接管，
 *   防选中切换时盒模型 ±2px 跳动；背景/文字色由 .ui-active !important 覆盖）
 * - active + ghost：不挂 soft-chip/ui-btn-*（ghost 本无边框，无盒模型差），
 *   让 ui-active 干净接管
 * - active + primary/danger：保留 variant 面（自身已带强语义色），ui-active 仍挂但仅做微提示
 *   （.ui-active 的染主色规则经 :not(.ui-btn-primary):not(.ui-btn-danger) 排除，不污染语义色）
 * - 非 active：default/danger → soft-chip；primary → ui-btn-primary；ghost → ui-btn-ghost
 */
const surfaceClass = computed(() => {
  if (props.active && props.variant === 'ghost') return ''
  return props.variant === 'default' || props.variant === 'danger' ? 'soft-chip' : ''
})

const variantClass = computed(() => {
  // active + ghost/default 时不挂 variant 类，让 ui-active 干净接管
  if (props.active && (props.variant === 'ghost' || props.variant === 'default')) return ''
  return variantClasses[props.variant]
})

const variantClasses: Record<NonNullable<Props['variant']>, string> = {
  default: '',
  primary: 'ui-btn-primary',
  ghost: 'ui-btn-ghost',
  danger: 'ui-btn-danger',
}
</script>
