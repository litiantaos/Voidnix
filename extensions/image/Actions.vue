<template>
  <BaseSelect :model-value="tool" :options="options" @update:model-value="handleChange" />
</template>

<script setup lang="ts">
import BaseSelect from '@/components/ui/BaseSelect.vue'
import { t } from '@/runtime/i18n'
import { tool, type Tool } from './index'

const options = [
  { label: t('image.removeBg'), value: 'removeBg' },
  { label: t('image.stitch'), value: 'stitch' },
]

function handleChange(val: string | number) {
  tool.value = val as Tool
}
</script>
