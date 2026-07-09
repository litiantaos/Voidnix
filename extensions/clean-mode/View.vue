<template>
  <div class="flex-col-full">
    <BaseSettingsList :items="items" />
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { CMD } from '@/commands'
import { useAppStore } from '@/stores/app'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import type { SettingItem } from '@/types/settings'

const isEnabled = ref(false)
const appStore = useAppStore()
let unlistenFn: UnlistenFn | undefined

const enable = async () => {
  if (isEnabled.value) return
  try {
    await invoke(CMD.setCleanModeEnabled, { enabled: true })
    isEnabled.value = true
  } catch (e) {
    appStore.showStatus(`${e ?? '未知错误'}`, { duration: 6000, kind: 'error' })
  }
}

onMounted(async () => {
  try {
    isEnabled.value = await invoke<boolean>(CMD.isCleanModeEnabled)
  } catch (e) {
    console.error('[clean-mode] isCleanModeEnabled failed:', e)
  }
  unlistenFn = await listen('clean-mode-exit', () => {
    isEnabled.value = false
  })
})

onBeforeUnmount(() => {
  unlistenFn?.()
})

const items = computed<SettingItem[]>(() => [
  {
    id: 'enable',
    title: '清洁模式',
    subtitle: '全屏黑屏、键鼠锁定，长按鼠标/触控板 2 秒退出',
    group: '通用',
    type: 'button',
    label: isEnabled.value ? '已开启' : '已关闭',
    variant: isEnabled.value ? 'primary' : 'default',
    action: enable,
  },
])
</script>
