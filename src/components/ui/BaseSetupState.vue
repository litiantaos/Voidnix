<template>
  <BaseEmptyState :icon="icon" :title="title" :description="description">
    <template #action>
      <BaseButton variant="primary" @click="emit('configure')">
        {{ actionText ?? t('common.goConfigure') }}
      </BaseButton>
    </template>
  </BaseEmptyState>
</template>

<script setup lang="ts">
import { t } from '@/runtime/i18n'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import BaseButton from '@/components/ui/BaseButton.vue'

withDefaults(
  defineProps<{
    title: string
    description?: string
    icon?: string
    /** 按钮文案；缺省统一「去配置」（ai-providers 中枢空态等语义不同时覆盖，如「添加提供商」） */
    actionText?: string
  }>(),
  { icon: 'i-ri-key-2-line' },
)

const emit = defineEmits<{ configure: [] }>()
</script>
