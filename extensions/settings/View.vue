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

/// 重看首启引导：置回 onboarded=false + 清 query + 回主界面（fullscreen 槽策略 watch
/// 据此激活）；写完结恢复目标（fullscreenReturnExtId）使引导 Esc/Enter 后回设置页
const handleShowWelcome = () => {
  appStore.fullscreenReturnExtId = 'settings'
  settings.onboarded = false
  appStore.setSearchQuery('')
  appStore.setActiveExtension(null)
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

const handleOpenWebsite = async () => {
  if (isTauri) {
    await open('https://voidnix.app')
  }
}

const handleOpenGitHub = async () => {
  if (isTauri) {
    await open('https://github.com/litiantaos/Voidnix')
  }
}

/// 清除 Voidnix shell 注入（.zshrc/.zprofile 注入块 + ai.env 凭证投影）。卸载导向的清理入口。
const handleClearInjections = async () => {
  if (!isTauri) return
  const confirmed = await appStore.showConfirm({
    title: t('settings.clearInjectionsConfirmTitle'),
    message: t('settings.clearInjectionsConfirmMessage'),
    size: 'md',
    markdown: true,
    okLabel: t('settings.clearInjectionsOk'),
  })
  if (!confirmed) return
  try {
    const cleared = await invoke<string[]>(CMD.clearVoidnixInjections)
    await appStore.showConfirm({
      title: t('settings.clearInjectionsConfirmTitle'),
      message:
        cleared.length > 0
          ? t('settings.clearInjectionsDone', { count: cleared.length })
          : t('settings.clearInjectionsNone'),
      showCancel: false,
      okLabel: t('settings.updateOK'),
    })
  } catch (e) {
    await appStore.showStatus(
      `${t('settings.clearInjectionsFailed')}：${e ?? t('common.unknownError')}`,
      {
        kind: 'error',
      },
    )
  }
}

/// 检查更新：立即弹 UpdateDialog、弹窗内检查（菜单栏「检查更新」与搜索角标同源入口）
const handleCheckUpdate = () => updateStore.startCheck()

function permStatus(granted: boolean | null): string {
  if (granted === null) return t('settings.permChecking')
  return granted ? t('settings.permGranted') : t('settings.permDenied')
}

/// 设备控制（辅助功能）API 请求（公证分流由唯一调用点承载：未公证直接发起授权会话）
async function handleRequestAccessibility() {
  if (!isTauri) return
  systemStore.permAccessibility = await invoke<boolean>(CMD.requestAccessibilityPermission)
}

/// 录屏：未公证直接发起授权会话（API 请求写入的 TCC 条目无效且污染列表）；
/// 公证版请求弹窗点允许后系统自行引导退出重开（预绑定权限需重启生效），
/// 未获准则会话直达系统设置
async function handleRequestScreenRecording() {
  if (!isTauri) return
  if (systemStore.appNotarized === true) {
    const granted = await invoke<boolean>(CMD.requestScreenRecordingPermission)
    if (granted) return
  }
  systemStore.startPermGrant('screen_recording')
}

/// 完全访问无 API 请求路径，恒为授权会话直达设置
function handleFullDiskAccess() {
  systemStore.startPermGrant('full_disk_access')
}

const allSettingsItems = computed<SettingItem[]>(() => {
  const items: SettingItem[] = []

  items.push({
    id: 'appearance',
    title: t('settings.appearance'),
    type: 'select',
    icon: 'i-ri-contrast-2-line',
    group: t('settings.group.general'),
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
    group: t('settings.group.general'),
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
    // 注册失败（被其它应用占用）时标红提示，配合启动时的改键引导
    subtitle: appStore.shortcutErrors.main ? t('settings.shortcutConflictHint') : undefined,
    tone: appStore.shortcutErrors.main ? 'danger' : undefined,
    type: 'shortcut',
    icon: 'i-ri-keyboard-line',
    group: t('settings.group.general'),
    value: settings.globalShortcut,
    update: handleGlobalShortcutChange,
  })

  items.push({
    id: 'autostart',
    title: t('settings.autostart'),
    type: 'toggle',
    icon: 'i-ri-shut-down-line',
    group: t('settings.group.general'),
    value: systemStore.autostartEnabled,
    update: handleAutostartToggle,
  })

  items.push({
    id: 'menubar-icon',
    title: t('settings.menubarIcon'),
    type: 'toggle',
    icon: 'i-ri-layout-top-line',
    group: t('settings.group.general'),
    value: settings.menubarIconVisible,
    update: (v: boolean) => {
      settings.menubarIconVisible = v
    },
  })

  items.push({
    id: 'show-welcome',
    title: t('settings.showWelcome'),
    type: 'action',
    icon: 'i-ri-guide-line',
    group: t('settings.group.general'),
    action: handleShowWelcome,
  })

  // 权限行顺序与引导面板一致：设备控制 → 完全访问，录屏（预绑定，授权后须重启）恒置末位
  items.push({
    id: 'perm-accessibility',
    title: t('settings.privacy.accessibility'),
    subtitle: permStatus(permAccessibility.value),
    type: 'action',
    icon: permAccessibility.value ? 'i-ri-checkbox-circle-line' : 'i-ri-alert-line',
    group: t('settings.group.privacy'),
    action: async () => {
      // 未授权且公证版才走 API 请求；其余路径（已授权仅查看 / 未公证手动添加）直达设置
      if (!permAccessibility.value && systemStore.appNotarized === true) {
        await handleRequestAccessibility()
      }
      systemStore.startPermGrant('accessibility')
    },
  })
  items.push({
    id: 'perm-full-disk-access',
    title: t('settings.privacy.fullDiskAccess'),
    subtitle: permStatus(permFullDiskAccess.value),
    type: 'action',
    icon: permFullDiskAccess.value ? 'i-ri-checkbox-circle-line' : 'i-ri-alert-line',
    group: t('settings.group.privacy'),
    action: () => handleFullDiskAccess(),
  })
  items.push({
    id: 'perm-screen-recording',
    title: t('settings.privacy.screenRecording'),
    subtitle: permStatus(permScreenRecording.value),
    type: 'action',
    icon: permScreenRecording.value ? 'i-ri-checkbox-circle-line' : 'i-ri-alert-line',
    group: t('settings.group.privacy'),
    action: () => {
      if (permScreenRecording.value) return systemStore.startPermGrant('screen_recording')
      return handleRequestScreenRecording()
    },
  })
  // 关于组：版本/更新与产品链接（版本信息天然属于「关于」）
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
    group: t('settings.group.about'),
    action: handleCheckUpdate,
  })

  items.push({
    id: 'website',
    title: t('settings.website'),
    type: 'action',
    icon: 'i-ri-global-line',
    subtitle: 'voidnix.app',
    group: t('settings.group.about'),
    action: handleOpenWebsite,
  })

  items.push({
    id: 'about',
    title: t('settings.about'),
    type: 'action',
    icon: 'i-ri-information-line',
    subtitle: 'github.com/litiantaos/Voidnix',
    group: t('settings.group.about'),
    action: handleOpenGitHub,
  })

  items.push({
    id: 'clear-injections',
    title: t('settings.clearInjections'),
    subtitle: t('settings.clearInjectionsHint'),
    type: 'action',
    icon: 'i-ri-eraser-line',
    group: t('settings.group.advanced'),
    action: handleClearInjections,
  })

  items.push({
    id: 'quit-app',
    title: t('settings.quit'),
    type: 'action',
    icon: 'i-ri-logout-box-line',
    group: t('settings.group.advanced'),
    action: handleQuitApp,
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
