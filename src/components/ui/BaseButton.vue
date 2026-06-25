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

const variantClasses: Record<string, string> = {
  default: 'hover:bg-black/8',
  primary: 'bg-accent text-white hover:bg-accent/90',
  outline: 'border border-solid border-black/12 bg-transparent text-tx-primary hover:bg-black/4',
  ghost: 'bg-transparent hover:bg-black/5',
  danger: 'text-red-500 hover:bg-red-500/10',
}
</script>
