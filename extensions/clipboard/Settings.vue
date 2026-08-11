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
import { t } from '@/runtime/i18n'

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
    title: t('clipboard.clearHistoryTitle'),
    message: t('clipboard.clearHistoryMessage'),
    okLabel: t('common.confirm'),
    cancelLabel: t('common.cancel'),
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
    title: t('clipboard.settings.shortcut'),
    type: 'shortcut',
    group: t('clipboard.settings.groupGeneral'),
    value: clipboardShortcutValue.value,
    update: handleClipboardShortcutChange,
  },
  {
    id: 'clipboard-maxdays',
    title: t('clipboard.settings.retention'),
    type: 'select',
    group: t('clipboard.settings.groupGeneral'),
    value: clipboardConfig.maxDays,
    options: [
      { label: t('clipboard.settings.days15'), value: 15 },
      { label: t('clipboard.settings.days30'), value: 30 },
      { label: t('clipboard.settings.days90'), value: 90 },
      { label: t('clipboard.settings.forever'), value: 0 },
    ],
    update: handleMaxDaysChange,
  },
  {
    id: 'clipboard-clear',
    title: t('clipboard.settings.clearUnfavorited'),
    type: 'button',
    group: t('clipboard.settings.groupData'),
    label: t('clipboard.settings.clear'),
    variant: 'danger',
    action: handleClearHistory,
  },
])
</script>
