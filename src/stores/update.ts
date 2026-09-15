import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { isTauri } from '@/utils/tauri'
import { getVersion } from '@tauri-apps/api/app'
import type { Update as TauriUpdate, DownloadEvent } from '@tauri-apps/plugin-updater'

export interface UpdateInfo {
  currentVersion: string
  newVersion: string
  body: string | null
}

export const useUpdateStore = defineStore('update', () => {
  const downloading = ref(false)
  const downloaded = ref(false)
  const checking = ref(false)
  const error = ref<string | null>(null)
  const info = ref<UpdateInfo | null>(null)
  const dialogVisible = ref(false)
  const progress = ref(0) // 0..1 下载进度（contentLength 未知时保持 0）
  // 当前应用版本：check() 时填充（无论有无更新），弹窗「已是最新版本」展示用
  const currentVersion = ref('')

  let _updater: TauriUpdate | null = null

  /** 同步菜单栏「检查更新」项文案：有新版本显示「更新到新版本（x.x.x）」，null 还原。 */
  function syncMenuLabel(version: string | null) {
    if (!isTauri) return
    invoke(CMD.setUpdateVersion, { version }).catch(() => {})
  }

  async function check(): Promise<boolean> {
    if (!isTauri) return false
    checking.value = true
    error.value = null
    try {
      const { check: checkUpdate } = await import('@tauri-apps/plugin-updater')
      currentVersion.value = await getVersion()
      const update = await checkUpdate()
      if (update?.available) {
        _updater = update
        info.value = {
          currentVersion: currentVersion.value,
          newVersion: update.version,
          body: update.body ?? null,
        }
        syncMenuLabel(update.version)
        return true
      }
      return false
    } catch (e) {
      error.value = String(e)
      return false
    } finally {
      checking.value = false
    }
  }

  async function download(): Promise<void> {
    if (!_updater || downloading.value || downloaded.value) return
    downloading.value = true
    error.value = null
    progress.value = 0
    try {
      let total = 0
      let received = 0
      // 仅下载更新包（不安装）；安装由 install() 单独触发，与弹窗的两步交互对应
      await _updater.download((e: DownloadEvent) => {
        if (e.event === 'Started' && e.data.contentLength) {
          total = e.data.contentLength
        } else if (e.event === 'Progress') {
          received += e.data.chunkLength
          if (total > 0) progress.value = Math.min(1, received / total)
        } else if (e.event === 'Finished') {
          progress.value = 1
        }
      })
      downloaded.value = true
    } catch (e) {
      error.value = String(e)
    } finally {
      downloading.value = false
    }
  }

  async function install(): Promise<void> {
    if (!_updater) return
    try {
      await _updater.install()
    } catch (e) {
      error.value = String(e)
    }
  }

  /** 主动检查更新统一入口（菜单栏 / 设置页 / 搜索角标 / 弹窗内重试共用）：
   *  立即弹窗承载全流程（检查中 → 结果），不阻塞等待网络检查；
   *  已有结果（发现更新/已下载）或下载进行中仅重新弹窗呈现，不重复发起检查。 */
  function startCheck() {
    if (checking.value || downloading.value || downloaded.value || info.value) {
      dialogVisible.value = true
      return
    }
    reset()
    dialogVisible.value = true
    void check()
  }

  function closeDialog() {
    dialogVisible.value = false
  }

  function reset() {
    downloading.value = false
    downloaded.value = false
    checking.value = false
    error.value = null
    info.value = null
    progress.value = 0
    currentVersion.value = ''
    _updater = null
    dialogVisible.value = false
    syncMenuLabel(null)
  }

  return {
    downloading,
    downloaded,
    checking,
    error,
    info,
    progress,
    dialogVisible,
    currentVersion,
    check,
    startCheck,
    download,
    install,
    closeDialog,
    reset,
  }
})
