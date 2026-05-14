<script setup lang="ts">
import { ref, computed } from 'vue'
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
import { useSettingsInput, type SettingItem } from '@/composables/useSettingsInput'

const settings = useSettingsStore()
const appStore = useAppStore()
const updateStore = useUpdateStore()
const { handleExecute, setShortcutRef } = useSettingsInput()

const query = computed(() => appStore.searchQuery.toLowerCase().trim())
const showUpdateDialog = ref(false)
const appVersion = ref('')

if (isTauri) {
  getVersion().then((v) => { appVersion.value = v }).catch(() => {})
}

function isVisible(...keywords: string[]) {
  if (!query.value) return true
  return keywords.some((k) => k.toLowerCase().includes(query.value))
}

const handleGlobalShortcutChange = async (val: string) => {
  await settings.setGlobalShortcut(val)
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
      okLabel: '好',
    })
  } else {
    await appStore.showConfirm({
      title: '检查更新失败',
      message: updateStore.error ?? '网络错误，请稍后重试。',
      showCancel: false,
      okLabel: '好',
    })
  }
}

const handleOpenGitHub = async () => {
  if (isTauri) {
    await open('https://github.com/litiantaos/Voidnix')
  }
}

const visibleItems = computed<SettingItem[]>(() => {
  const items: SettingItem[] = []

  if (isVisible('应用', 'app', '快捷键', 'shortcut', 'keyboard', '唤醒')) {
    items.push({
      id: 'app-shortcut',
      title: '启动快捷键',
      type: 'shortcut',
      value: settings.globalShortcut,
      update: handleGlobalShortcutChange,
    })
  }

  if (isVisible('版本', 'version', '关于', 'about', 'github', '更新', 'update')) {
    items.push({
      id: 'app-version',
      title: '当前版本',
      type: 'button',
      icon: 'i-ri-information-line',
      value: appVersion.value ? `v${appVersion.value}` : '',
      action: handleOpenGitHub,
    })
  }

  if (isVisible('更新', 'update', '版本', 'version', '检查')) {
    const checkLabel = updateStore.checking
      ? '检查中…'
      : updateStore.downloaded
        ? '有新版本，点击安装'
        : '检查更新'
    items.push({
      id: 'check-update',
      title: checkLabel,
      type: 'button',
      icon: updateStore.downloaded ? 'i-ri-arrow-up-circle-line' : 'i-ri-refresh-line',
      value: '',
      action: handleCheckUpdate,
    })
  }

  if (isVisible('退出', 'quit', 'exit', '关闭', 'close')) {
    items.push({
      id: 'quit-app',
      title: '退出应用',
      type: 'button',
      value: '',
      action: handleQuitApp,
    })
  }

  return items
})

const selectedIndex = ref(0)
</script>

<template>
  <div class="pb-4 flex flex-col h-full">
    <BaseList
      v-if="visibleItems.length > 0"
      :items="visibleItems"
      v-model:selected-index="selectedIndex"
      keyboard-navigation
      @execute="(item: SettingItem, _i: number, e?: KeyboardEvent) => handleExecute(item, e)"
    >
      <template #item="{ item, selected, hoverable, setRef, select }">
        <BaseListItem
          :ref="setRef"
          :id="`set-${item.id}`"
          :title="item.title"
          :icon="item.icon"
          :hoverable="hoverable"
          :selected="selected"
          @click="select"
          @dblclick="handleExecute(item)"
        >
          <template v-if="item.type === 'shortcut'" #trailing>
            <ShortcutInput
              :ref="(el: any) => setShortcutRef(`si-${item.id}`, el)"
              :model-value="String(item.value)"
              @update:model-value="item.update"
            />
          </template>
          <template v-else-if="item.id === 'app-version' && appVersion" #trailing>
            <span class="text-xs text-tx-muted font-mono">v{{ appVersion }}</span>
          </template>
        </BaseListItem>
      </template>
    </BaseList>

    <BaseEmptyState
      v-else
      icon="i-ri-search-line"
      title="没有找到相关设置"
    />
  </div>

  <UpdateDialog v-if="showUpdateDialog" @close="showUpdateDialog = false" />
</template>
