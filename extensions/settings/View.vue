<template>
  <div class="flex-col-full-pb">
    <BaseSettingsList v-if="visibleItems.length > 0" :items="visibleItems" shortcut-id="main" />

    <BaseEmptyState v-else icon="i-ri-search-line" :title="t('settings.noResultsFound')" />
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
import type { Appearance, Language } from '@/stores/settings'
import { isTauri } from '@/utils/tauri'
import { scoreFields } from '@/utils/fuzzy'
import { t } from '@/runtime/i18n'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import type { SettingItem } from '@/types/settings'

const settings = useSettingsStore()
const appStore = useAppStore()
const updateStore = useUpdateStore()
const systemStore = useSystemStore()

const query = computed(() => appStore.searchQuery.toLowerCase().trim())
const appVersion = ref('')

const permScreenRecording = computed(() => systemStore.permScreenRecording)
const permAccessibility = computed(() => systemStore.permAccessibility)
const permFullDiskAccess = computed(() => systemStore.permFullDiskAccess)

const handleAutostartToggle = async (val: boolean) => {
  if (!isTauri) return
  try {
    await invoke<void>(val ? CMD.enableAutostart : CMD.disableAutostart)
    systemStore.autostartEnabled = val
  } catch (e) {
    await appStore.showStatus(`${t('settings.autostart')}：${e ?? t('common.operationFailed')}`, {
      kind: 'error',
    })
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
    title: t('settings.quitConfirmTitle'),
    message: t('settings.quitConfirmMessage'),
    okLabel: t('settings.quitLabel'),
    cancelLabel: t('common.cancel'),
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
  if (updateStore.info) {
    updateStore.showDialog()
    return
  }
  updateStore.reset()
  const hasUpdate = await updateStore.check()
  if (hasUpdate) {
    updateStore.showDialog()
  } else if (!updateStore.error) {
    await appStore.showConfirm({
      title: t('settings.checkUpdate'),
      message: t('settings.upToDate', { version: appVersion.value }),
      showCancel: false,
      okLabel: t('settings.updateOK'),
    })
  } else {
    await appStore.showConfirm({
      title: t('settings.checkUpdate'),
      message: updateStore.error ?? t('common.networkError'),
      showCancel: false,
      okLabel: t('settings.updateOK'),
    })
  }
}

function permStatus(granted: boolean | null): string {
  if (granted === null) return t('settings.permChecking')
  return granted ? t('settings.permGranted') : t('settings.permDenied')
}

async function handleRequestAccessibility() {
  if (!isTauri) return
  systemStore.permAccessibility = await invoke<boolean>(CMD.requestAccessibilityPermission)
}

async function handleOpenPrivacy(kind: string) {
  if (!isTauri) return
  await invoke(CMD.openPrivacySettings, { kind })
}

const allSettingsItems = computed<SettingItem[]>(() => {
  const items: SettingItem[] = []

  items.push({
    id: 'appearance',
    title: t('settings.appearance'),
    type: 'select',
    icon: 'i-ri-contrast-2-line',
    group: t('settings.group.app'),
    value: settings.appearance,
    options: [
      { label: t('settings.appearance.auto'), value: 'auto' },
      { label: t('settings.appearance.light'), value: 'light' },
      { label: t('settings.appearance.dark'), value: 'dark' },
    ],
    update: (v: string | number) => {
      settings.appearance = v as Appearance
    },
  })

  items.push({
    id: 'language',
    title: t('settings.language'),
    type: 'select',
    icon: 'i-ri-translate-2',
    group: t('settings.group.app'),
    value: settings.language,
    options: [
      { label: t('settings.language.zh-CN'), value: 'zh-CN' },
      { label: t('settings.language.en'), value: 'en' },
    ],
    update: (v: string | number) => {
      settings.language = v as Language
    },
  })

  items.push({
    id: 'app-shortcut',
    title: t('settings.shortcut'),
    type: 'shortcut',
    icon: 'i-ri-keyboard-line',
    group: t('settings.group.app'),
    value: settings.globalShortcut,
    update: handleGlobalShortcutChange,
  })

  items.push({
    id: 'autostart',
    title: t('settings.autostart'),
    type: 'toggle',
    icon: 'i-ri-shut-down-line',
    group: t('settings.group.app'),
    value: systemStore.autostartEnabled,
    update: handleAutostartToggle,
  })

  const checkLabel = updateStore.checking
    ? t('settings.checking')
    : updateStore.downloading
      ? t('settings.downloading')
      : updateStore.downloaded
        ? t('settings.installUpdate')
        : updateStore.info
          ? t('settings.downloadAndInstall')
          : t('settings.checkUpdate')
  let versionLabel = appVersion.value ? `v${appVersion.value}` : ''
  if (updateStore.info) {
    versionLabel = `→ ${updateStore.info.newVersion}（v${updateStore.info.currentVersion}）`
  }
  items.push({
    id: 'check-update',
    title: checkLabel,
    subtitle: versionLabel,
    type: 'action',
    icon: updateStore.downloaded ? 'i-ri-arrow-up-circle-line' : 'i-ri-refresh-line',
    group: t('settings.group.app'),
    action: handleCheckUpdate,
  })

  items.push({
    id: 'about',
    title: t('settings.about'),
    type: 'action',
    icon: 'i-ri-information-line',
    subtitle: 'github.com/litiantaos/Voidnix',
    group: t('settings.group.app'),
    action: handleOpenGitHub,
  })

  items.push({
    id: 'quit-app',
    title: t('settings.quit'),
    type: 'action',
    icon: 'i-ri-logout-box-line',
    group: t('settings.group.app'),
    action: handleQuitApp,
  })

  items.push({
    id: 'perm-screen-recording',
    title: t('settings.privacy.screenRecording'),
    subtitle: permStatus(permScreenRecording.value),
    type: 'action',
    icon: permScreenRecording.value ? 'i-ri-checkbox-circle-line' : 'i-ri-alert-line',
    group: t('settings.group.privacy'),
    action: () => handleOpenPrivacy('screen_recording'),
  })
  items.push({
    id: 'perm-accessibility',
    title: t('settings.privacy.accessibility'),
    subtitle: permStatus(permAccessibility.value),
    type: 'action',
    icon: permAccessibility.value ? 'i-ri-checkbox-circle-line' : 'i-ri-alert-line',
    group: t('settings.group.privacy'),
    action: async () => {
      if (!permAccessibility.value) {
        await handleRequestAccessibility()
      }
      await handleOpenPrivacy('accessibility')
    },
  })
  items.push({
    id: 'perm-full-disk-access',
    title: t('settings.privacy.fullDiskAccess'),
    subtitle: permStatus(permFullDiskAccess.value),
    type: 'action',
    icon: permFullDiskAccess.value ? 'i-ri-checkbox-circle-line' : 'i-ri-alert-line',
    group: t('settings.group.privacy'),
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
