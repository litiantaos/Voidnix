<template>
  <BaseButton
    :icon="appStore.activeSubview === 'config' ? 'i-ri-settings-3-fill' : 'i-ri-settings-3-line'"
    @click="toggleConfig"
  />
</template>

<script setup lang="ts">
import BaseButton from '@/components/ui/BaseButton.vue'
import { useAppStore } from '@/stores/app'

const appStore = useAppStore()

function toggleConfig() {
  if (appStore.activeSubview === 'config') appStore.closeSubview()
  else appStore.openSubview('config')
}
</script>
