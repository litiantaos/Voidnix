import { defineStore } from 'pinia'
import { ref } from 'vue'
import { isTauri } from '@/utils/tauri'
import type { Update as TauriUpdate } from '@tauri-apps/plugin-updater'

export interface UpdateInfo {
  currentVersion: string
  newVersion: string
  body: string | null
}

export const useUpdateStore = defineStore('update', () => {
  const available = ref(false)
  const downloading = ref(false)
  const downloaded = ref(false)
  const checking = ref(false)
  const error = ref<string | null>(null)
  const info = ref<UpdateInfo | null>(null)
  const dialogVisible = ref(false)

  let _updater: TauriUpdate | null = null

  async function check(): Promise<boolean> {
    if (!isTauri) return false
    checking.value = true
    error.value = null
    try {
      const { check: checkUpdate } = await import('@tauri-apps/plugin-updater')
      const { getVersion } = await import('@tauri-apps/api/app')
      const update = await checkUpdate()
      if (update?.available) {
        _updater = update
        info.value = {
          currentVersion: await getVersion(),
          newVersion: update.version,
          body: update.body ?? null,
        }
        available.value = true
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
    try {
      await _updater.downloadAndInstall((progress: { event: string }) => {
        if (progress.event === 'Finished') {
          downloaded.value = true
        }
      })
      // downloadAndInstall 在 macOS 上会直接安装并重启，
      // 若走到这里说明只下载完成还未重启
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
    available.value = false
    downloading.value = false
    downloaded.value = false
    checking.value = false
    error.value = null
    info.value = null
    _updater = null
    dialogVisible.value = false
  }

  return {
    available,
    downloading,
    downloaded,
    checking,
    error,
    info,
    dialogVisible,
    check,
    download,
    install,
    showDialog,
    closeDialog,
    reset,
  }
})
