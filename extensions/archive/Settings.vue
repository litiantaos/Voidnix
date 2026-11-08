<template>
  <div class="flex-col-view">
    <BaseSettingsList :items="items" />
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { withSuppressBlur } from '@/stores/app'
import { t } from '@/runtime/i18n'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import type { SettingItem } from '@/types/settings'
import { config } from './config'

async function pickDir(key: 'compressDir') {
  await withSuppressBlur(async () => {
    const selected = await invoke<string>(CMD.pickDirectory)
    if (selected) config[key] = selected
  })
}

const items = computed<SettingItem[]>(() => {
  const group = t('archive.settings.group')
  const list: SettingItem[] = [
    {
      id: 'compressDir',
      title: t('archive.settings.compressDir'),
      subtitle: config.compressDir || t('archive.settings.compressDirHint'),
      type: 'action',
      action: () => void pickDir('compressDir'),
      group,
    },
  ]
  if (config.compressDir) {
    list.push({
      id: 'clearDirs',
      title: t('archive.clear'),
      type: 'button',
      variant: 'default',
      action: () => {
        config.compressDir = ''
      },
      group,
    })
  }
  return list
})
</script>
