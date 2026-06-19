import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'

/** 写文本到剪贴板（走 platform::pasteboard::write_text，替代 tauri-plugin-clipboard-manager）。 */
export function writeText(value: string): Promise<void> {
  return invoke(CMD.pasteboardWriteText, { text: value })
}
