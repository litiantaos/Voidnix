<template>
  <div class="flex-col-full-pb">
    <BaseSettingsList :items="items" shortcut-id="screenshot" />
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { withSuppressBlur } from '@/stores/app'
import { config as screenshotConfig } from './config'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import type { SettingItem } from '@/types/settings'
import { useShortcutConfig } from '@/composables/useShortcutConfig'

const { value: screenshotShortcutValue, update: handleShortcutChange } = useShortcutConfig(
  'screenshot',
  'Alt+S',
)

async function pickSavePath() {
  const selected = await withSuppressBlur(() => invoke<string>(CMD.pickDirectory))
  if (selected) {
    screenshotConfig.savePath = selected
  }
}

function savePathDisplay(path: string): string {
  if (!path) return '~/Downloads'
  return path.replace(/^\/Users\/[^/]+/, '~')
}

const items = computed<SettingItem[]>(() => [
  {
    id: 'screenshot-shortcut',
    title: '启动快捷键',
    type: 'shortcut',
    value: screenshotShortcutValue.value,
    update: handleShortcutChange,
    group: '通用',
  },
  {
    id: 'savePath',
    title: '截图保存位置',
    subtitle: savePathDisplay(screenshotConfig.savePath),
    type: 'action',
    action: pickSavePath,
    group: '通用',
  },
])
</script>
