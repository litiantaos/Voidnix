<template>
  <div class="flex-col-full-pb">
    <BaseSettingsList :items="items" shortcut-id="screenshot" />
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { config as screenshotConfig } from './config'
import { useAppStore } from '@/stores/app'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import type { SettingItem } from '@/types/settings'
import { useShortcutConfig } from '@/composables/useShortcutConfig'

const appStore = useAppStore()

const { value: screenshotShortcutValue, update: handleShortcutChange } = useShortcutConfig(
  'screenshot',
  'Alt+S',
)

async function pickSavePath() {
  // NSOpenPanel 运行期间抑制失焦隐藏
  appStore.suppressBlur = true
  try {
    const selected = await invoke<string>(CMD.pickDirectory)
    if (selected) {
      await (screenshotConfig.savePath = selected)
    }
  } finally {
    setTimeout(() => {
      appStore.suppressBlur = false
    }, 800)
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
