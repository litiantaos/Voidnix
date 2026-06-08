<template>
  <component v-if="view" :is="componentMap[view.type]" :view="view" @action="onAction" />
  <BaseEmptyState v-else text="无内容" />
</template>

<script setup lang="ts">
import type { Component } from 'vue'
import type { View } from '@/types/declarative'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import DeclarativeList from './DeclarativeList.vue'
import DeclarativeMarkdown from './DeclarativeMarkdown.vue'
import DeclarativeForm from './DeclarativeForm.vue'
import DeclarativeDetail from './DeclarativeDetail.vue'
import DeclarativeStream from './DeclarativeStream.vue'

defineProps<{ view: View | null }>()

const emit = defineEmits<{
  action: [actionId: string, payload: Record<string, unknown>]
}>()

const componentMap: Record<string, Component> = {
  list: DeclarativeList,
  markdown: DeclarativeMarkdown,
  form: DeclarativeForm,
  detail: DeclarativeDetail,
  stream: DeclarativeStream,
}

function onAction(actionId: string, payload: Record<string, unknown>) {
  emit('action', actionId, payload)
}
</script>
