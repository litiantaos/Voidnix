import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { hideWindow } from '@/utils/tauri'

export async function copyAndHide(value: string) {
  await writeText(value)
  hideWindow()
}
