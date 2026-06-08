<template>
  <div text="tx-muted" flex="~ col 1" gap="3" select="none" items="center" justify="center">
    <slot name="icon">
      <i v-if="loading" class="i-ri-loader-4-line animate-spin" text="2xl" opacity="50"></i>
      <i v-else-if="icon" :class="icon" text="2xl" opacity="50"></i>
    </slot>
    <div text="sm" font="medium">
      <slot>{{ loading ? '加载中' : title }}</slot>
    </div>
    <div v-if="description" text="xs tx-faint">
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
