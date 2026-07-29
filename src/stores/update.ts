import { defineStore } from 'pinia'
import { ref } from 'vue'
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

  let _updater: TauriUpdate | null = null

  async function check(): Promise<boolean> {
    if (!isTauri) return false
    checking.value = true
    error.value = null
    try {
      const { check: checkUpdate } = await import('@tauri-apps/plugin-updater')
      const update = await checkUpdate()
      if (update?.available) {
        _updater = update
        info.value = {
          currentVersion: await getVersion(),
          newVersion: update.version,
          body: update.body ?? null,
        }
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

  function showDialog() {
    dialogVisible.value = true
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
    _updater = null
    dialogVisible.value = false
  }

  return {
    downloading,
    downloaded,
    checking,
    error,
    info,
    progress,
    dialogVisible,
    check,
    download,
    install,
    showDialog,
    closeDialog,
    reset,
  }
})
