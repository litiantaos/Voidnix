<template>
  <BaseButton
    :icon="activeTab === 'favorites' ? 'i-ri-star-fill text-amber-400' : 'i-ri-star-line'"
    @click="toggleFavoriteTab"
  />
</template>

<script setup lang="ts">
import { nextTick } from 'vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import { activeTab } from './index'

function toggleFavoriteTab() {
  activeTab.value = activeTab.value === 'all' ? 'favorites' : 'all'
  // 切标签后焦点回到搜索框：避免焦点留在按钮导致后续回车重复触发收藏切换，
  // 用户切完标签即可继续方向键导航 / 回车粘贴列表项
  nextTick(() => document.getElementById('main-search-input')?.focus())
}
</script>
