<template>
  <BaseButton size="icon" @click="toggleFavoriteTab">
    <div
      :class="activeTab === 'favorites' ? 'i-ri-star-fill text-amber-400' : 'i-ri-star-line'"
      class="text-sm"
    ></div>
  </BaseButton>
  <BaseButton size="icon" @click="appStore.togglePanel()">
    <div class="i-ri-settings-3-line text-sm"></div>
  </BaseButton>
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
