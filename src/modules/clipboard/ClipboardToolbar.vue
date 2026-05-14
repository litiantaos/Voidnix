<script setup lang="ts">
import { useAppStore } from '@/stores/app'
import BaseButton from '@/components/ui/BaseButton.vue'

const appStore = useAppStore()
</script>

<template>
  <BaseButton size="icon" @click="appStore.toggleSettings()">
    <div class="i-ri-settings-3-line text-sm"></div>
  </BaseButton>
</template>
