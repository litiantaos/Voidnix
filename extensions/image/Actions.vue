<template>
  <BaseButton
    :icon="appStore.activeSubview === 'config' ? 'i-ri-settings-3-fill' : 'i-ri-settings-3-line'"
    :title="appStore.activeSubview === 'config' ? t('image.closeSettings') : t('image.settings')"
    :aria-label="
      appStore.activeSubview === 'config' ? t('image.closeSettings') : t('image.settings')
    "
    @click="toggleConfig"
  />
</template>

<script setup lang="ts">
import { useAppStore } from '@/stores/app'
import { t } from '@/runtime/i18n'
import BaseButton from '@/components/ui/BaseButton.vue'

const appStore = useAppStore()

function toggleConfig() {
  if (appStore.activeSubview === 'config') appStore.closeSubview()
  else appStore.openSubview('config')
}
</script>
