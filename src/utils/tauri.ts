import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'

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
  if (auto) {
    invoke(CMD.hideWindow, { auto: true }).catch(() => {})
  } else {
    invoke(CMD.hideWindow).catch(() => {})
  }
}

export const isTauri =
  typeof window !== 'undefined' &&
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ !== undefined
