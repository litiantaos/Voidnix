<script setup lang="ts">
import { activeTab } from './index'
</script>

<template>
  <div class="flex gap-4">
    <button
      class="font-medium transition-colors"
      :class="
        activeTab === 'all'
          ? 'text-accent'
          : 'text-black/40 hover:text-black/60'
      "
      @click="activeTab = 'all'"
    >
      全部
    </button>
    <button
      class="font-medium transition-colors"
      :class="
        activeTab === 'favorites'
          ? 'text-accent'
          : 'text-black/40 hover:text-black/60'
      "
      @click="activeTab = 'favorites'"
    >
      收藏
    </button>
  </div>
</template>
