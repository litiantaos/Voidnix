<template>
  <BaseSelect :model-value="tool" :options="options" @update:model-value="handleChange" />
</template>

<script setup lang="ts">
import BaseSelect from '@/components/ui/BaseSelect.vue'
import { tool, type Tool } from './index'

const options = [
  { label: '移除背景', value: 'removeBg' },
  { label: '拼接长图', value: 'stitch' },
]

function handleChange(val: string | number) {
  tool.value = val as Tool
}
</script>
