<template>
  <div class="flex-col-full-pb">
    <BaseSettingsList :items="items" shortcut-id="clipboard" />
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useAppStore } from '@/stores/app'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { config as clipboardConfig } from './config'
import { invalidateCache, fetchClipboardHistory, activeTab } from './index'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import type { SettingItem } from '@/types/settings'
import { useShortcutConfig } from '@/composables/useShortcutConfig'

const appStore = useAppStore()

const { value: clipboardShortcutValue, update } = useShortcutConfig('clipboard', 'Alt+C')

const handleClipboardShortcutChange = (val: string | number) => update(String(val))

const handleMaxDaysChange = async (val: string | number) => {
  const n = val as number
  if (!isNaN(n)) {
    clipboardConfig.maxDays = n
  }
}

const handleClearHistory = async () => {
  const confirmed = await appStore.showConfirm({
    title: '清空剪贴板记录',
    message: '确定要清空所有未收藏的剪贴板记录吗？',
    kind: 'warning',
    okLabel: '确定',
    cancelLabel: '取消',
  })

  if (confirmed) {
    try {
      await invoke(CMD.clearClipboardHistory)
      invalidateCache()
      await fetchClipboardHistory('', activeTab.value === 'favorites')
    } catch (e) {
      console.error('Failed to clear clipboard history:', e)
    }
  }
}

const items = computed<SettingItem[]>(() => [
  {
    id: 'clipboard-shortcut',
    title: '启动快捷键',
    type: 'shortcut',
    group: '通用',
    value: clipboardShortcutValue.value,
    update: handleClipboardShortcutChange,
  },
  {
    id: 'clipboard-maxdays',
    title: '记录保留时长',
    type: 'select',
    group: '通用',
    value: clipboardConfig.maxDays,
    options: [
      { label: '15 天', value: 15 },
      { label: '30 天', value: 30 },
      { label: '90 天', value: 90 },
      { label: '永久', value: 0 },
    ],
    update: handleMaxDaysChange,
  },
  {
    id: 'clipboard-clear',
    title: '清空未收藏记录',
    type: 'button',
    group: '数据',
    label: '清空',
    action: handleClearHistory,
  },
])
</script>
