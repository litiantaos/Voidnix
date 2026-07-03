import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { clearToasts } from '@/composables/useToast'

/** Rust search_apps/search_files 返回的原始结构（serde 形状，手写） */
export interface RawSearchResult {
  id: string
  title: string
  path: string
  kind: string
  icon: string | null
  last_used: string | null
  score: number | null
  use_count: number | null
  parent: string | null
}

export function hideWindow(auto = false) {
  // 窗口隐藏即清 toast：macOS 隐藏 WebView 会节流 setTimeout，未清的 toast 会残留到下次显示
  clearToasts()
  if (auto) {
    invoke(CMD.hideWindow, { auto: true }).catch(() => {})
  } else {
    invoke(CMD.hideWindow).catch(() => {})
  }
}

export const isTauri =
  typeof window !== 'undefined' &&
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ !== undefined
