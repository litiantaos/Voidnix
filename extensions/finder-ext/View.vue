<template>
  <div class="flex-col-full-pb">
    <BaseSettingsList :items="allItems" />
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { config as finderConfig } from './config'
import { useAppStore } from '@/stores/app'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import type { SettingItem } from '@/types/settings'

const appStore = useAppStore()

const authorized = ref<boolean | null>(null)

async function checkAuthorized() {
  try {
    authorized.value = await invoke<boolean>(CMD.checkFinderExtAuthorized)
  } catch {
    authorized.value = false
  }
}

checkAuthorized()

const toggle = async (next: boolean) => {
  // H11：显式 invoke 同步 Rust 状态；成功才回写 config，失败给用户反馈
  try {
    await invoke(CMD.setFinderExtEnabled, { enabled: next })
  } catch (e) {
    appStore.showStatus(`开关失败：${e ?? '未知错误'}`, { duration: 4000, kind: 'error' })
    return
  }
  finderConfig.enabled = next
  if (next && !authorized.value) {
    try {
      await invoke(CMD.openExtensionsPrefs)
    } catch (e) {
      console.error('Failed to open extensions prefs:', e)
    }
  }
}

const allItems = computed<SettingItem[]>(() => [
  {
    id: 'enable',
    title: '启用访达右键菜单',
    subtitle: authorized.value === false ? '开启后将引导你到系统设置中启用扩展' : undefined,
    type: 'toggle',
    value: finderConfig.enabled,
    update: toggle,
    group: '通用',
  },
  { id: 'copy-path', title: '拷贝路径', type: 'custom', group: '菜单项' },
  { id: 'open-terminal', title: '在终端中打开', type: 'custom', group: '菜单项' },
  { id: 'new-file', title: '新建文件', type: 'custom', group: '菜单项' },
  { id: 'toggle-hidden', title: '切换隐藏文件', type: 'custom', group: '菜单项' },
])
</script>
