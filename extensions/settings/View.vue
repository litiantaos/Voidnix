<template>
  <div class="pb-4 flex flex-col h-full">
    <BaseList
      v-if="visibleItems.length > 0"
      :items="visibleItems"
      v-model:selected-index="selectedIndex"
      group-field="group"
      :group-title="(g: string) => g"
      keyboard-navigation
      @execute="
        (item: SettingItem, _i: number, e?: KeyboardEvent) =>
          handleExecute(item, e)
      "
    >
      <template #item="{ item, selected, setRef, select }">
        <BaseListItem
          :ref="setRef"
          :id="`set-${item.id}`"
          :title="item.title"
          :subtitle="item.subtitle"
          :icon="item.icon"
          :selected="selected"
          @click="select"
          @dblclick="handleExecute(item)"
        >
          <template v-if="item.type === 'shortcut'" #trailing>
            <ShortcutInput
              :model-value="String(item.value)"
              @update:model-value="item.update"
            />
          </template>
        </BaseListItem>
      </template>
    </BaseList>

    <BaseEmptyState v-else icon="i-ri-search-line" title="没有找到相关设置" />
  </div>

  <UpdateDialog v-if="showUpdateDialog" @close="showUpdateDialog = false" />
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { getVersion } from '@tauri-apps/api/app'
import { open } from '@tauri-apps/plugin-shell'
import { useSettingsStore } from '@/stores/settings'
import { useAppStore } from '@/stores/app'
import { useUpdateStore } from '@/stores/update'
import { isTauri } from '@/utils/tauri'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import ShortcutInput from '@/components/ui/ShortcutInput.vue'
import UpdateDialog from '@/components/ui/UpdateDialog.vue'
import {
  useSettingsInput,
  type SettingItem,
} from '@/composables/useSettingsInput'

const settings = useSettingsStore()
const appStore = useAppStore()
const updateStore = useUpdateStore()
const { handleExecute } = useSettingsInput()

const query = computed(() => appStore.searchQuery.toLowerCase().trim())
const showUpdateDialog = ref(false)
const appVersion = ref('')

const permScreenRecording = ref<boolean | null>(null)
const permAccessibility = ref<boolean | null>(null)
const permFullDiskAccess = ref<boolean | null>(null)

async function refreshPermissions() {
  if (!isTauri) return
  try {
    permScreenRecording.value = await invoke<boolean>('check_screen_recording_permission')
    permAccessibility.value = await invoke<boolean>('check_accessibility_permission')
    permFullDiskAccess.value = await invoke<boolean>('check_full_disk_access_permission')
  } catch {
    // 权限检查失败（非 macOS 或不支持），保持 null
  }
}

onMounted(refreshPermissions)

if (isTauri) {
  getVersion()
    .then((v) => {
      appVersion.value = v
    })
    .catch(() => {})
}

function isVisible(...keywords: string[]) {
  if (!query.value) return true
  return keywords.some((k) => k.toLowerCase().includes(query.value))
}

const handleGlobalShortcutChange = async (val: string | number) => {
  await settings.setGlobalShortcut(val as string)
}

const handleQuitApp = async () => {
  const confirmed = await appStore.showConfirm({
    title: '退出应用',
    message: '确定要退出 Voidnix 吗？所有访达扩展也将一并停止。',
    okLabel: '退出',
    cancelLabel: '取消',
    kind: 'warning',
  })
  if (confirmed) {
    await invoke('quit_app')
  }
}

const handleOpenGitHub = async () => {
  if (isTauri) {
    await open('https://github.com/litiantaos/Voidnix')
  }
}

const handleCheckUpdate = async () => {
  if (updateStore.downloaded) {
    showUpdateDialog.value = true
    return
  }
  updateStore.reset()
  const hasUpdate = await updateStore.check()
  if (hasUpdate) {
    await updateStore.download()
    showUpdateDialog.value = true
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
  const granted = await invoke<boolean>('request_accessibility_permission')
  permAccessibility.value = granted
}

async function handleOpenPrivacy(kind: string) {
  if (!isTauri) return
  await invoke('open_privacy_settings', { kind })
  // 返回刷新状态，给系统一点时间响应
  setTimeout(refreshPermissions, 1000)
}

const visibleItems = computed<SettingItem[]>(() => {
  const items: SettingItem[] = []

  if (isVisible('应用', 'app', '快捷键', 'shortcut', 'keyboard', '唤醒')) {
    items.push({
      id: 'app-shortcut',
      title: '启动快捷键',
      type: 'shortcut',
      icon: 'i-ri-keyboard-line',
      group: '应用',
      value: settings.globalShortcut,
      update: handleGlobalShortcutChange,
    })
  }

  if (isVisible('更新', 'update', '版本', 'version', '检查', '应用', 'app')) {
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
      type: 'button',
      icon: updateStore.downloaded
        ? 'i-ri-arrow-up-circle-line'
        : 'i-ri-refresh-line',
      group: '应用',
      value: '',
      action: handleCheckUpdate,
    })
  }

  if (isVisible('关于', 'about', 'github', '项目', 'project')) {
    items.push({
      id: 'about',
      title: '关于',
      type: 'button',
      icon: 'i-ri-information-line',
      subtitle: 'github.com/litiantaos/Voidnix',
      group: '应用',
      value: '',
      action: handleOpenGitHub,
    })
  }

  if (isVisible('退出', 'quit', 'exit', '关闭', 'close')) {
    items.push({
      id: 'quit-app',
      title: '退出应用',
      type: 'button',
      icon: 'i-ri-logout-box-line',
      group: '应用',
      value: '',
      action: handleQuitApp,
    })
  }

  if (isVisible('权限', '隐私', '录制', '辅助', '磁盘', 'accessibility', 'screen', 'disk', 'privacy')) {
    items.push({
      id: 'perm-screen-recording',
      title: '屏幕录制权限',
      subtitle: permStatus(permScreenRecording.value),
      type: 'button',
      icon: permScreenRecording.value ? 'i-ri-checkbox-circle-line' : 'i-ri-alert-line',
      group: '隐私权限',
      value: '',
      action: () => handleOpenPrivacy('screen_recording'),
    })
    items.push({
      id: 'perm-accessibility',
      title: '辅助功能权限',
      subtitle: permStatus(permAccessibility.value),
      type: 'button',
      icon: permAccessibility.value ? 'i-ri-checkbox-circle-line' : 'i-ri-alert-line',
      group: '隐私权限',
      value: '',
      action: () => {
        if (permAccessibility.value) {
          handleOpenPrivacy('accessibility')
        } else {
          handleRequestAccessibility()
        }
      },
    })
    items.push({
      id: 'perm-full-disk-access',
      title: '完全磁盘访问权限',
      subtitle: permStatus(permFullDiskAccess.value),
      type: 'button',
      icon: permFullDiskAccess.value ? 'i-ri-checkbox-circle-line' : 'i-ri-alert-line',
      group: '隐私权限',
      value: '',
      action: () => handleOpenPrivacy('full_disk_access'),
    })
  }

  return items
})

const selectedIndex = ref(0)
</script>
