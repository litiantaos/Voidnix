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

/** 显示主窗口（扩展快捷键从隐藏呼出：先切视图再 show，避免渲染旧视图闪现）。 */
export function showWindow() {
  // 两层 rAF：等 Vue 视图切换的 DOM 更新 + WKWebView paint 完成一帧，
  // 再发起 show。NSWindow orderFront 会立即合成 WKWebView 的 layer（最近 commit 的 bitmap），
  // 若 paint 未完成则合成上一帧（主界面），表现为闪现。两层 rAF 确保 show 时已是新帧。
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      invoke(CMD.showWindow).catch(() => {})
    })
  })
}

export const isTauri =
  typeof window !== 'undefined' &&
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ !== undefined
