<script setup lang="ts">
interface Props {
  variant?: 'default' | 'primary' | 'outline'
  size?: 'default' | 'icon'
  active?: boolean
  disabled?: boolean
}

withDefaults(defineProps<Props>(), {
  variant: 'default',
  size: 'default',
  active: false,
  disabled: false,
})

const sizeClasses: Record<string, string> = {
  default: '',
  icon: 'w-7 flex items-center justify-center px-0 text-sm',
}

const variantClasses: Record<string, string> = {
  default: 'hover:bg-black/8',
  primary: 'bg-accent text-white hover:bg-accent/90',
  outline:
    'border border-solid border-black/12 bg-transparent text-tx-primary hover:bg-black/4',
}
</script>

<template>
  <button
    :disabled="disabled"
    :class="[
      'ui-ctrl flex-none',
      sizeClasses[size],
      variantClasses[variant],
      
      disabled ? 'ui-disabled' : '',
    ]"
  >
    <slot />
  </button>
</template>
