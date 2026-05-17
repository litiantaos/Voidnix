<template>
  <div
    class="text-tx-muted flex flex-1 flex-col gap-3 select-none items-center justify-center"
  >
    <slot name="icon">
      <i
        v-if="loading"
        class="i-ri-loader-4-line text-2xl opacity-50 animate-spin"
      ></i>
      <i v-else-if="icon" :class="icon" class="text-2xl opacity-50"></i>
    </slot>
    <div class="text-sm font-medium">
      <slot>{{ loading ? '加载中' : title }}</slot>
    </div>
    <div v-if="description" class="text-xs text-tx-faint">
      {{ description }}
    </div>
  </div>
</template>

<script setup lang="ts">
defineProps<{
  icon?: string
  title?: string
  description?: string
  loading?: boolean
}>()
</script>
