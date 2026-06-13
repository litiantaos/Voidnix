import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { hideWindow } from '@/utils/tauri'
import { useAppStore } from '@/stores/app'

let hideTimer: ReturnType<typeof setTimeout> | null = null

export async function copyAndShow(value: string, label = '已复制') {
  await writeText(value)
  useAppStore().showStatus(label, 2000)
}

export async function copyAndHide(value: string, label = '已复制') {
  if (hideTimer) {
    clearTimeout(hideTimer)
    hideTimer = null
  }
  await writeText(value)
  useAppStore().showStatus(label, 800)
  hideTimer = setTimeout(() => {
    hideTimer = null
    hideWindow()
  }, 800)
}
