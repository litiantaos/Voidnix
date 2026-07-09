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
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import type { SettingItem } from '@/types/settings'

const appStore = useAppStore()

const toggle = async (next: boolean) => {
  try {
    await invoke(CMD.setZshAutosuggestionsEnabled, { enabled: next })
    zshConfig.enabled = next
  } catch (e) {
    appStore.showStatus(`启用失败：${String(e ?? '未知错误')}`, { duration: 4000, kind: 'error' })
  }
}

const items = computed<SettingItem[]>(() => [
  {
    id: 'enable',
    title: '启用终端自动建议',
    subtitle: 'Tab 切换备选，→ 接受，Ctrl+X 开关，Ctrl+C 清空',
    type: 'toggle',
    value: zshConfig.enabled,
    update: toggle,
    group: '通用',
  },
])
</script>
