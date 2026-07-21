<template>
  <BaseSelect
    :model-value="activeType"
    :options="typeOptions"
    @update:model-value="handleTypeChange"
  />
  <BaseButton
    :icon="activeTab === 'favorites' ? 'i-ri-star-fill text-warning' : 'i-ri-star-line'"
    @click="toggleFavoriteTab"
  />
  <BaseButton
    :icon="appStore.activeSubview === 'config' ? 'i-ri-settings-3-fill' : 'i-ri-settings-3-line'"
    @click="toggleConfig"
  />
</template>

<script setup lang="ts">
import { nextTick } from 'vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseSelect from '@/components/ui/BaseSelect.vue'
import { useAppStore, focusSearchInput } from '@/stores/app'
import { activeTab, activeType } from './index'
import type { ContentType } from './logic'

const appStore = useAppStore()

const typeOptions = [
  { label: '全部', value: 'all' },
  { label: '文本', value: 'text' },
  { label: '图片', value: 'image' },
  { label: '文件', value: 'file' },
]

function handleTypeChange(val: string | number) {
  activeType.value = val as ContentType
  nextTick(() => focusSearchInput())
}

function toggleFavoriteTab() {
  activeTab.value = activeTab.value === 'all' ? 'favorites' : 'all'
  nextTick(() => focusSearchInput())
}

function toggleConfig() {
  if (appStore.activeSubview === 'config') appStore.closeSubview()
  else appStore.openSubview('config')
}
</script>
