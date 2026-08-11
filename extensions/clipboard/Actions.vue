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
import { computed, nextTick } from 'vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseSelect from '@/components/ui/BaseSelect.vue'
import { useAppStore, focusSearchInput } from '@/stores/app'
import { activeTab, activeType } from './index'
import type { ContentType } from './logic'
import { t } from '@/runtime/i18n'

const appStore = useAppStore()

const typeOptions = computed(() => [
  { label: t('clipboard.filter.all'), value: 'all' },
  { label: t('clipboard.filter.text'), value: 'text' },
  { label: t('clipboard.filter.image'), value: 'image' },
  { label: t('clipboard.filter.file'), value: 'file' },
])

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
