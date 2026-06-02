<template>
  <BaseButton
    :icon="activeTab === 'favorites' ? 'i-ri-star-fill text-amber-400' : 'i-ri-star-line'"
    @click="toggleFavoriteTab"
  />
  <BaseButton
    :icon="appStore.activeSubview === 'settings' ? 'i-ri-settings-3-fill' : 'i-ri-settings-3-line'"
    @click="appStore.toggleSubview('settings')"
  />
</template>

<script setup lang="ts">
import { useAppStore } from '@/stores/app'
import BaseButton from '@/components/ui/BaseButton.vue'
import { activeTab } from './index'

const appStore = useAppStore()

function toggleFavoriteTab() {
  activeTab.value = activeTab.value === 'all' ? 'favorites' : 'all'
}
</script>
