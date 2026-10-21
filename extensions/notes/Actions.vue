<template>
  <BaseButton
    :icon="previewMode ? 'i-ri-eye-fill text-accent' : 'i-ri-eye-line'"
    :title="previewMode ? t('notes.edit') : t('notes.preview')"
    :aria-label="previewMode ? t('notes.edit') : t('notes.preview')"
    @click="togglePreview"
  />
  <BaseButton
    :icon="appStore.activeSubview === 'config' ? 'i-ri-settings-3-fill' : 'i-ri-settings-3-line'"
    @click="toggleConfig"
  />
</template>

<script setup lang="ts">
import BaseButton from '@/components/ui/BaseButton.vue'
import { useAppStore } from '@/stores/app'
import { t } from '@/runtime/i18n'
import { previewMode } from './view-mode'

const appStore = useAppStore()

function togglePreview() {
  previewMode.value = !previewMode.value
}

function toggleConfig() {
  if (appStore.activeSubview === 'config') appStore.closeSubview()
  else appStore.openSubview('config')
}
</script>
