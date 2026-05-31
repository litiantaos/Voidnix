<template>
  <BaseButton
    :icon="appStore.activePanel === 'settings' ? 'i-ri-settings-3-fill' : 'i-ri-settings-3-line'"
    @click="appStore.togglePanel('settings')"
  />
</template>

<script setup lang="ts">
import { useAppStore } from '@/stores/app'
import BaseButton from '@/components/ui/BaseButton.vue'

const appStore = useAppStore()
</script>
