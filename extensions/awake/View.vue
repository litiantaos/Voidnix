<template>
  <div class="flex-col-full">
    <BaseSettingsList :items="items" />
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { CMD } from '@/commands'
import { useAppStore } from '@/stores/app'
import { t } from '@/runtime/i18n'
import { config as awakeConfig } from './config'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import type { SettingItem } from '@/types/settings'

const isEnabled = ref(false)
const appStore = useAppStore()
let unlistenEnabled: (() => void) | undefined
let unlistenMode: (() => void) | undefined

const checkStatus = async () => {
  try {
    isEnabled.value = await invoke<boolean>(CMD.isAwakeEnabled)
  } catch (e) {
    console.error('Failed to check awake status:', e)
  }
}

const toggleAwake = async (next: boolean) => {
  try {
    await invoke(CMD.setAwakeEnabled, { enabled: next })
    isEnabled.value = next
  } catch (e) {
    appStore.showStatus(`${t('common.operationFailed')}: ${e ?? t('common.unknownError')}`, {
      duration: 4000,
      kind: 'error',
    })
  }
}

const onModeChange = (value: string | number) => {
  awakeConfig.displayMode = value as typeof awakeConfig.displayMode
}

onMounted(async () => {
  checkStatus()
  // 菜单栏操作改状态后 Rust emit 同步面板显示（与 proxy-enabled 同模式）
  unlistenEnabled = await listen<boolean>('awake-enabled', (e) => {
    isEnabled.value = e.payload
  })
  unlistenMode = await listen<string>('awake-mode', (e) => {
    awakeConfig.displayMode = e.payload as typeof awakeConfig.displayMode
  })
})

onUnmounted(() => {
  unlistenEnabled?.()
  unlistenMode?.()
})

const items = computed<SettingItem[]>(() => [
  {
    id: 'awake',
    title: t('awake.enable'),
    subtitle: t('awake.enableHint'),
    type: 'toggle',
    value: isEnabled.value,
    update: toggleAwake,
    group: t('awake.group.display'),
  },
  {
    id: 'mode',
    title: t('awake.displayMode'),
    subtitle: t('awake.displayModeHint'),
    type: 'select',
    value: awakeConfig.displayMode,
    options: [
      { label: t('awake.mirror'), value: 'mirror' },
      { label: t('awake.extend'), value: 'extend' },
    ],
    update: onModeChange,
    group: t('awake.group.display'),
  },
])
</script>
