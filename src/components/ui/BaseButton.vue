<template>
  <button
    type="button"
    :disabled="disabled"
    :class="[
      'ui-ctrl flex-none',
      isIconOnly ? 'w-7 px-0 flex-center' : icon ? 'flex gap-1.5 items-center' : '',
      surfaceClass,
      variantClass,
      disabled ? 'ui-disabled' : '',
      active ? 'ui-active' : '',
    ]"
  >
    <i v-if="icon" :class="[icon, 'text-sm']" />
    <slot />
  </button>
</template>

<script setup lang="ts">
import { computed, useSlots } from 'vue'

interface Props {
  variant?: 'default' | 'primary' | 'ghost' | 'danger'
  disabled?: boolean
  icon?: string
  /** 显式 active 状态（列表/弹窗焦点环视觉提示，避免作为 fallthrough attr 污染 DOM） */
  active?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  variant: 'default',
  disabled: false,
  active: false,
})

const slots = useSlots()
const isIconOnly = computed(() => !!props.icon && !slots.default)

/**
 * 面选择：
 * - active + ghost/default：挂 ui-active（浅底 + 主色文字/图标），跳过 soft-chip/ui-btn-*
 *   避免其 background !important 覆盖 ui-active
 * - active + primary/danger：保留 variant 面（自身已带强语义色），ui-active 仍挂但仅做微提示
 *   （.ui-active 的染主色规则经 :not(.ui-btn-primary):not(.ui-btn-danger) 排除，不污染语义色）
 * - 非 active：default/danger → soft-chip；primary → ui-btn-primary；ghost → ui-btn-ghost
 */
const surfaceClass = computed(() => {
  if (props.active && (props.variant === 'ghost' || props.variant === 'default')) return ''
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
