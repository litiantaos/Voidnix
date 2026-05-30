import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { getCurrentWindow } from '@tauri-apps/api/window'

export async function copyAndHide(value: string) {
  await writeText(value)
  await getCurrentWindow().hide()
}
