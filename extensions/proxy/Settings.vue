<template>
  <div class="flex-col-full-pb">
    <!-- 状态未就绪/下载中不渲染（防闪空态/误报「未安装」）；无足迹显示空态 -->
    <BaseSettingsList v-if="statusLoaded && items.length > 0" :items="items" />
    <BaseEmptyState
      v-else-if="statusLoaded && !footprint.downloading"
      :title="t('proxy.noCoreFootprint')"
      icon="i-ri-inbox-archive-line"
    />
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
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import type { SettingItem } from '@/types/settings'

/// 代理设置子视图（config subview）：完全卸载入口。
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
  // 无核心且无 daemon = 无系统足迹可清理（空态）；下载中不展示（在飞下载会复活卸载产物）
  if (!footprint.value.downloaded && !footprint.value.daemonInstalled) return []
  if (footprint.value.downloading) return []
  return [
    {
      id: 'proxy-uninstall',
      title: t('proxy.uninstall'),
      subtitle: t('proxy.uninstallHint'),
      type: 'button',
      label: t('proxy.uninstallConfirmOk'),
      variant: 'danger',
      group: t('proxy.settingsGroup'),
      action: uninstall,
    },
  ]
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
