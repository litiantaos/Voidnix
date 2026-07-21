<template>
  <div class="flex-col-full-pb">
    <BaseSettingsList v-if="visibleItems.length > 0" :items="visibleItems" shortcut-id="main" />

    <BaseEmptyState v-else icon="i-ri-search-line" title="没有找到相关设置" />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { getVersion } from '@tauri-apps/api/app'
import { open } from '@tauri-apps/plugin-shell'
import { useSettingsStore } from '@/stores/settings'
import { useAppStore } from '@/stores/app'
import { useUpdateStore } from '@/stores/update'
import { isTauri } from '@/utils/tauri'
import { scoreFields } from '@/utils/fuzzy'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import type { SettingItem } from '@/types/settings'

const settings = useSettingsStore()
const appStore = useAppStore()
const updateStore = useUpdateStore()

const query = computed(() => appStore.searchQuery.toLowerCase().trim())
const appVersion = ref('')

const permScreenRecording = ref<boolean | null>(null)
const permAccessibility = ref<boolean | null>(null)
const permFullDiskAccess = ref<boolean | null>(null)

async function refreshPermissions() {
  if (!isTauri) return
  try {
    permScreenRecording.value = await invoke<boolean>(CMD.checkScreenRecordingPermission)
    permAccessibility.value = await invoke<boolean>(CMD.checkAccessibilityPermission)
    permFullDiskAccess.value = await invoke<boolean>(CMD.checkFullDiskAccessPermission)
  } catch {}
}

onMounted(refreshPermissions)

if (isTauri) {
  getVersion()
    .then((v) => {
      appVersion.value = v
    })
    .catch(() => {})
}

const handleGlobalShortcutChange = async (val: string | number) => {
  await settings.setGlobalShortcut(val as string)
}

const handleQuitApp = async () => {
  const confirmed = await appStore.showConfirm({
    title: '退出应用',
    message: '确定要退出 Voidnix 吗？',
    okLabel: '退出',
    cancelLabel: '取消',
  })
  if (confirmed) {
    await invoke(CMD.quitApp)
  }
}

const handleOpenGitHub = async () => {
  if (isTauri) {
    await open('https://github.com/litiantaos/Voidnix')
  }
}

const handleCheckUpdate = async () => {
  if (updateStore.downloaded) {
    updateStore.showDialog()
    return
  }
  updateStore.reset()
  const hasUpdate = await updateStore.check()
  if (hasUpdate) {
    await updateStore.download()
    updateStore.showDialog()
  } else if (!updateStore.error) {
    await appStore.showConfirm({
      title: '已是最新版本',
      message: `当前版本 v${appVersion.value} 已是最新版本。`,
      showCancel: false,
      okLabel: '好的',
    })
  } else {
    await appStore.showConfirm({
      title: '检查更新失败',
      message: updateStore.error ?? '网络错误，请稍后重试。',
      showCancel: false,
      okLabel: '好的',
    })
  }
}

function permStatus(granted: boolean | null): string {
  if (granted === null) return '检查中…'
  return granted ? '已授权' : '未授权 — 点击前往系统设置'
}

async function handleRequestAccessibility() {
  if (!isTauri) return
  const granted = await invoke<boolean>(CMD.requestAccessibilityPermission)
  permAccessibility.value = granted
}

async function handleOpenPrivacy(kind: string) {
  if (!isTauri) return
  await invoke(CMD.openPrivacySettings, { kind })
  setTimeout(refreshPermissions, 1000)
}

const allSettingsItems = computed<SettingItem[]>(() => {
  const items: SettingItem[] = []

  items.push({
    id: 'app-shortcut',
    title: '启动快捷键',
    type: 'shortcut',
    icon: 'i-ri-keyboard-line',
    group: '应用',
    value: settings.globalShortcut,
    update: handleGlobalShortcutChange,
  })

  const checkLabel = updateStore.checking
    ? '检查中…'
    : updateStore.downloaded
      ? '有新版本，点击安装'
      : '检查更新'
  let versionLabel = appVersion.value ? `当前版本：${appVersion.value}` : ''
  if (updateStore.downloaded && updateStore.info) {
    versionLabel = `新版本：${updateStore.info.newVersion}（当前版本：${updateStore.info.currentVersion}）`
  }
  items.push({
    id: 'check-update',
    title: checkLabel,
    subtitle: versionLabel,
    type: 'action',
    icon: updateStore.downloaded ? 'i-ri-arrow-up-circle-line' : 'i-ri-refresh-line',
    group: '应用',
    action: handleCheckUpdate,
  })

  items.push({
    id: 'about',
    title: '关于',
    type: 'action',
    icon: 'i-ri-information-line',
    subtitle: 'github.com/litiantaos/Voidnix',
    group: '应用',
    action: handleOpenGitHub,
  })

  items.push({
    id: 'quit-app',
    title: '退出应用',
    type: 'action',
    icon: 'i-ri-logout-box-line',
    group: '应用',
    action: handleQuitApp,
  })

  items.push({
    id: 'perm-screen-recording',
    title: '屏幕录制权限',
    subtitle: permStatus(permScreenRecording.value),
    type: 'action',
    icon: permScreenRecording.value ? 'i-ri-checkbox-circle-line' : 'i-ri-alert-line',
    group: '隐私权限',
    action: () => handleOpenPrivacy('screen_recording'),
  })
  items.push({
    id: 'perm-accessibility',
    title: '辅助功能权限',
    subtitle: permStatus(permAccessibility.value),
    type: 'action',
    icon: permAccessibility.value ? 'i-ri-checkbox-circle-line' : 'i-ri-alert-line',
    group: '隐私权限',
    action: async () => {
      // 未授权时先触发系统把本应用注册进辅助功能列表（否则面板里找不到），再打开设置面板
      if (!permAccessibility.value) {
        await handleRequestAccessibility()
      }
      await handleOpenPrivacy('accessibility')
    },
  })
  items.push({
    id: 'perm-full-disk-access',
    title: '完全磁盘访问权限',
    subtitle: permStatus(permFullDiskAccess.value),
    type: 'action',
    icon: permFullDiskAccess.value ? 'i-ri-checkbox-circle-line' : 'i-ri-alert-line',
    group: '隐私权限',
    action: () => handleOpenPrivacy('full_disk_access'),
  })

  return items
})

const visibleItems = computed<SettingItem[]>(() => {
  const q = query.value
  if (!q) return allSettingsItems.value
  return allSettingsItems.value
    .map((item) => ({ item, score: scoreFields([item.title ?? '', item.subtitle ?? ''], q) }))
    .filter((x) => x.score > 0)
    .sort((a, b) => b.score - a.score)
    .map((x) => x.item)
})
</script>
