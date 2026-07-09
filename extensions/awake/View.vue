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
    appStore.showStatus(`切换失败：${e ?? '未知错误'}`, { duration: 4000, kind: 'error' })
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
    title: '启用唤醒',
    subtitle: '通过虚拟外接显示器触发 Clamshell Mode，需接入电源',
    type: 'toggle',
    value: isEnabled.value,
    update: toggleAwake,
    group: '显示器',
  },
  {
    id: 'mode',
    title: '显示模式',
    subtitle: '镜像与主屏显示相同画面，扩展提供独立桌面空间',
    type: 'select',
    value: awakeConfig.displayMode,
    options: [
      { label: '镜像', value: 'mirror' },
      { label: '扩展', value: 'extend' },
    ],
    update: onModeChange,
    group: '显示器',
  },
])
</script>
