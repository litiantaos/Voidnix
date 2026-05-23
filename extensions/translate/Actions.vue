<template>
  <BaseButton icon="i-ri-settings-3-line" @click="appStore.togglePanel()" />
</template>

<script setup lang="ts">
import { useAppStore } from '@/stores/app'
import BaseButton from '@/components/ui/BaseButton.vue'

const appStore = useAppStore()
</script>
