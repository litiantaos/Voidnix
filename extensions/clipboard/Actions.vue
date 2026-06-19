<template>
  <BaseButton
    :icon="activeTab === 'favorites' ? 'i-ri-star-fill text-amber-400' : 'i-ri-star-line'"
    @click="toggleFavoriteTab"
  />
</template>

<script setup lang="ts">
import BaseButton from '@/components/ui/BaseButton.vue'
import { activeTab } from './index'

function toggleFavoriteTab() {
  activeTab.value = activeTab.value === 'all' ? 'favorites' : 'all'
}
</script>
