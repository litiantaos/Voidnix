<template>
  <div class="flex-col-full-pb">
    <BaseSettingsList :items="items" />
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { config as zshConfig } from './config'
import { useAppStore } from '@/stores/app'
import { t } from '@/runtime/i18n'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import type { SettingItem } from '@/types/settings'

const appStore = useAppStore()

const toggle = async (next: boolean) => {
  try {
    await invoke(CMD.setZshAutosuggestionsEnabled, { enabled: next })
    zshConfig.enabled = next
  } catch (e) {
    appStore.showStatus(
      `${t('common.operationFailed')}: ${String(e ?? t('common.unknownError'))}`,
      {
        duration: 4000,
        kind: 'error',
      },
    )
  }
}

const items = computed<SettingItem[]>(() => [
  {
    id: 'enable',
    title: t('zshAutosuggestions.enable'),
    subtitle: t('zshAutosuggestions.hint'),
    type: 'toggle',
    value: zshConfig.enabled,
    update: toggle,
    group: t('common.group.general'),
  },
])
</script>
