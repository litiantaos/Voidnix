<template>
  <div text="muted" flex="~ col 1" gap="3" select="none" items="center" justify="center">
    <slot name="icon">
      <i v-if="loading" class="i-ri-loader-4-line animate-spin" text="2xl" opacity="50"></i>
      <i v-else-if="icon" :class="icon" text="2xl" opacity="50"></i>
    </slot>
    <div v-if="title || $slots.default" text="sm" font="medium">
      <slot>{{ loading ? t('common.loading') : title }}</slot>
    </div>
    <div v-if="description" text="xs muted">
      {{ description }}
    </div>
  </div>
</template>

<script setup lang="ts">
import { t } from '@/runtime/i18n'

defineProps<{
  icon?: string
  title?: string
  description?: string
  loading?: boolean
}>()
</script>
