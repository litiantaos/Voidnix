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

const checkStatus = async () => {
  try {
    isEnabled.value = await invoke<boolean>(CMD.isAwakeEnabled)
  } catch (e) {
    console.error('Failed to check awake status:', e)
  }
}

const toggleAwake = async (next: boolean) => {
  try {
    // 先 invoke 再写 config：成功路径由 watch 回声幂等处理，
    // 失败（授权取消等）时 config.enabled 未被置位，状态不漂移
    await invoke(CMD.setAwakeEnabled, { enabled: next })
    isEnabled.value = next
    awakeConfig.enabled = next
  } catch (e) {
    appStore.showStatus(`${t('common.operationFailed')}: ${e ?? t('common.unknownError')}`, {
      duration: 4000,
      kind: 'error',
    })
  }
}

onMounted(async () => {
  checkStatus()
  // 菜单栏操作改状态后 Rust emit 同步面板显示（与 proxy-enabled 同模式）
  unlistenEnabled = await listen<boolean>('awake-enabled', (e) => {
    isEnabled.value = e.payload
  })
})

onUnmounted(() => {
  unlistenEnabled?.()
})

const items = computed<SettingItem[]>(() => [
  {
    id: 'awake',
    title: t('awake.enable'),
    subtitle: t('awake.enableHint'),
    type: 'toggle',
    value: isEnabled.value,
    update: toggleAwake,
    group: t('awake.group.general'),
  },
  {
    id: 'menubar',
    title: t('awake.menubarToggle'),
    type: 'toggle',
    value: awakeConfig.menubarToggleVisible,
    update: (visible: boolean | string | number) => {
      awakeConfig.menubarToggleVisible = visible as boolean
    },
    group: t('awake.group.general'),
  },
])
</script>
