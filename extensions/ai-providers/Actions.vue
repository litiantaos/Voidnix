<template>
  <BaseButton icon="i-ri-add-line" title="添加提供商" aria-label="添加提供商" @click="onAdd" />
</template>

<script setup lang="ts">
import BaseButton from '@/components/ui/BaseButton.vue'
import { requestCreateProvider } from './bridge'

function onAdd() {
  requestCreateProvider()
}
</script>
