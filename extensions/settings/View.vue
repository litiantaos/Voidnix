<template>
  <div class="flex-col-full-pb">
    <BaseSettingsList v-if="visibleItems.length > 0" :items="visibleItems" shortcut-id="main" />

    <BaseEmptyState v-else icon="i-ri-search-line" title="没有找到相关设置" />
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { getVersion } from '@tauri-apps/api/app'
import { open } from '@tauri-apps/plugin-shell'
import { useSettingsStore } from '@/stores/settings'
import { useAppStore } from '@/stores/app'
import { useUpdateStore } from '@/stores/update'
import { useSystemStore } from '@/stores/system'
import type { Appearance } from '@/stores/settings'
import { isTauri } from '@/utils/tauri'
import { scoreFields } from '@/utils/fuzzy'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import type { SettingItem } from '@/types/settings'

const settings = useSettingsStore()
const appStore = useAppStore()
const updateStore = useUpdateStore()
const systemStore = useSystemStore()

const query = computed(() => appStore.searchQuery.toLowerCase().trim())
const appVersion = ref('')

// 权限/自启状态来自 systemStore（启动预查缓存）：设置页零 IPC、零首帧跳变。
// 刷新仅限 handleOpenPrivacy（用户从系统设置返回）——权限变更的唯一入口。
const permScreenRecording = computed(() => systemStore.permScreenRecording)
const permAccessibility = computed(() => systemStore.permAccessibility)
const permFullDiskAccess = computed(() => systemStore.permFullDiskAccess)

const handleAutostartToggle = async (val: boolean) => {
  if (!isTauri) return
  try {
    await invoke<void>(val ? CMD.enableAutostart : CMD.disableAutostart)
    systemStore.autostartEnabled = val
  } catch (e) {
    // 透传 macOS NSError 描述：dev 裸二进制无 bundle id / 系统拒绝等真实原因
    await appStore.showStatus(`开机自启：${e ?? '操作失败'}`, { kind: 'error' })
  }
}

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
  // 已知有更新（后台检查发现）/ 已下载 / 下载中：直接弹窗驱动后续交互
  if (updateStore.info) {
    updateStore.showDialog()
    return
  }
  updateStore.reset()
  const hasUpdate = await updateStore.check()
  if (hasUpdate) {
    // 发现更新即弹窗，下载与进度由 UpdateDialog 驱动（用户决定何时下载）
    updateStore.showDialog()
  } else if (!updateStore.error) {
    await appStore.showConfirm({
      title: '检查更新',
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
  systemStore.permAccessibility = await invoke<boolean>(CMD.requestAccessibilityPermission)
}

async function handleOpenPrivacy(kind: string) {
  if (!isTauri) return
  // 打开系统设置面板；权限状态刷新由 useAppLifecycle 的窗口获焦钩子统一处理
  // （用户改完权限返回时窗口重获焦点触发 refresh，比固定延时更可靠）
  await invoke(CMD.openPrivacySettings, { kind })
}

const allSettingsItems = computed<SettingItem[]>(() => {
  const items: SettingItem[] = []

  items.push({
    id: 'appearance',
    title: '外观',
    type: 'select',
    icon: 'i-ri-contrast-2-line',
    group: '应用',
    value: settings.appearance,
    options: [
      { label: '自动', value: 'auto' },
      { label: '浅色', value: 'light' },
      { label: '深色', value: 'dark' },
    ],
    update: (v: string | number) => {
      settings.appearance = v as Appearance
    },
  })

  items.push({
    id: 'app-shortcut',
    title: '启动快捷键',
    type: 'shortcut',
    icon: 'i-ri-keyboard-line',
    group: '应用',
    value: settings.globalShortcut,
    update: handleGlobalShortcutChange,
  })

  items.push({
    id: 'autostart',
    title: '开机自启',
    type: 'toggle',
    icon: 'i-ri-shut-down-line',
    group: '应用',
    value: systemStore.autostartEnabled,
    update: handleAutostartToggle,
  })

  const checkLabel = updateStore.checking
    ? '检查中…'
    : updateStore.downloading
      ? '下载中…'
      : updateStore.downloaded
        ? '安装新版本'
        : updateStore.info
          ? '下载并安装'
          : '检查更新'
  let versionLabel = appVersion.value ? `当前版本：${appVersion.value}` : ''
  if (updateStore.info) {
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
