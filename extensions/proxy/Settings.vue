<template>
  <div class="flex-col-full-pb">
    <BaseSettingsList v-if="statusLoaded" :items="items" />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onActivated } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { useAppStore } from '@/stores/app'
import { isTauri } from '@/utils/tauri'
import { t } from '@/runtime/i18n'
import { toErrorMessage } from '@/utils/format'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import { config, AUTO_UPDATE_INTERVAL_OPTIONS } from './config'
import type { SettingItem } from '@/types/settings'

/// 代理设置子视图（config subview）：菜单栏常显开关 + 订阅自动更新开关/间隔 + 完全卸载入口。
/// 核心状态自管（挂载/激活时拉权威值）——主视图与子视图互不感知，主视图靠
/// onActivated 对账 + proxy-enabled 事件同步（见 useProxyPanel）。
const appStore = useAppStore()

const statusLoaded = ref(false)
const uninstalling = ref(false)
const footprint = ref({ downloaded: false, daemonInstalled: false, downloading: false })

async function loadStatus() {
  if (!isTauri) {
    statusLoaded.value = true
    return
  }
  try {
    footprint.value = await invoke<{
      downloaded: boolean
      daemonInstalled: boolean
      downloading: boolean
    }>(CMD.proxyCoreStatus)
  } catch {
    /* keep last known */
  } finally {
    statusLoaded.value = true
  }
}

const items = computed<SettingItem[]>(() => {
  // 菜单栏常显开关恒在（显示偏好，与核心安装状态无关）
  const list: SettingItem[] = [
    {
      id: 'proxy-menubar',
      title: t('proxy.menubarToggle'),
      type: 'toggle',
      value: config.menubarVisible,
      update: (visible: boolean | string | number) => {
        config.menubarVisible = visible as boolean
      },
      group: t('proxy.settingsGroupGeneral'),
    },
    {
      id: 'proxy-auto-update',
      title: t('proxy.autoUpdate'),
      subtitle: t('proxy.autoUpdateHint'),
      type: 'toggle',
      value: config.autoUpdateEnabled,
      update: (enabled: boolean | string | number) => {
        config.autoUpdateEnabled = enabled as boolean
      },
      group: t('proxy.settingsGroupSubscription'),
    },
    {
      id: 'proxy-auto-update-interval',
      title: t('proxy.autoUpdateInterval'),
      type: 'select',
      value: config.autoUpdateIntervalHours,
      options: AUTO_UPDATE_INTERVAL_OPTIONS.map((h) => ({
        label: t('proxy.intervalHours', { n: h }),
        value: h,
      })),
      update: (v: string | number) => {
        config.autoUpdateIntervalHours = Number(v)
      },
      group: t('proxy.settingsGroupSubscription'),
    },
  ]
  // 完全卸载：有系统足迹且非下载中才展示（在飞下载会复活卸载产物）
  if (
    (footprint.value.downloaded || footprint.value.daemonInstalled) &&
    !footprint.value.downloading
  ) {
    list.push({
      id: 'proxy-uninstall',
      title: t('proxy.uninstall'),
      subtitle: t('proxy.uninstallHint'),
      type: 'button',
      label: t('proxy.uninstallConfirmOk'),
      variant: 'danger',
      group: t('proxy.settingsGroup'),
      action: uninstall,
    })
  }
  return list
})

/// 完全卸载：停代理 + 提权卸载 LaunchDaemon + 清理核心运行文件（订阅/端口配置保留）。
async function uninstall() {
  if (uninstalling.value) return
  const confirmed = await appStore.showConfirm({
    title: t('proxy.uninstallTitle'),
    message: t('proxy.uninstallConfirmMessage'),
    // lg：首项 /Library/LaunchDaemons 行内代码 + 后续文案合计约 545px，
    // md（~400px 内容宽）下断在「（root」后把括号短语劈成两行
    size: 'lg',
    markdown: true,
    okLabel: t('proxy.uninstallConfirmOk'),
  })
  if (!confirmed) return
  uninstalling.value = true
  try {
    await invoke(CMD.proxyUninstall)
    await loadStatus()
    appStore.showStatus(t('proxy.uninstalled'), { duration: 3000 })
  } catch (e) {
    // 提权取消/失败：核心文件可能仍在，刷新权威状态
    await loadStatus()
    appStore.showStatus(toErrorMessage(e, t('proxy.uninstallFailed')), {
      duration: 4000,
      kind: 'error',
    })
  } finally {
    uninstalling.value = false
  }
}

onActivated(() => {
  loadStatus()
})
</script>
