<template>
  <button
    type="button"
    :disabled="disabled"
    :class="[
      'ui-ctrl flex-none',
      isIconOnly
        ? 'w-7 px-0 text-sm flex items-center justify-center'
        : icon
          ? 'flex gap-1.5 items-center'
          : '',
      surfaceClass,
      variantClasses[variant],
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
  variant?: 'default' | 'primary' | 'outline' | 'ghost' | 'danger'
  disabled?: boolean
  icon?: string
  /** H9：显式 active 状态（焦点环视觉提示，避免作为 fallthrough attr 污染 DOM） */
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
 * 面：default / outline → soft-chip（outline 与 default 同面，API 保留）。
 * primary → .ui-btn-primary；ghost / danger → 透明底（不挂 chip）。
 */
const surfaceClass = computed(() =>
  props.variant === 'default' || props.variant === 'outline' ? 'soft-chip' : '',
)

const variantClasses: Record<string, string> = {
  default: '',
  primary: 'ui-btn-primary',
  outline: '',
  ghost:
    '!border-transparent !bg-transparent !shadow-none hover:!bg-[var(--color-fill-5)] [backdrop-filter:none]',
  danger:
    '!border-transparent !bg-transparent !shadow-none text-danger hover:!bg-danger-soft [backdrop-filter:none]',
}
</script>
