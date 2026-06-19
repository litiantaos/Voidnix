import { invoke } from '@tauri-apps/api/core'
import { hideWindow } from '@/utils/tauri'
import { useAppStore } from '@/stores/app'

let hideTimer: ReturnType<typeof setTimeout> | null = null

/** 写文本到剪贴板（走 platform::pasteboard::write_text，替代 tauri-plugin-clipboard-manager）。 */
export function writeText(value: string): Promise<void> {
  return invoke('pasteboard_write_text', { text: value })
}

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
