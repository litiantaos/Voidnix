import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
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
  // 通知前端释放结果列表 DOM（隐藏后不可见，DOM 节点作为 zombie 占内存直至下次 GC；
  // 唤起时 loadDefaultResults 走应用缓存毫秒级重载，无感知）
  window.dispatchEvent(new CustomEvent('window-hiding'))
  if (auto) {
    invoke(CMD.hideWindow, { auto: true }).catch(() => {})
  } else {
    invoke(CMD.hideWindow).catch(() => {})
  }
}

/** 显示主窗口（从隐藏呼出：先切视图再 show，避免渲染旧视图闪现）。 */
export async function showWindow() {
  // NSWindow.isVisible 区分两种状态（JS 事件循环 / IPC 不受窗口冻结影响，仅 rAF 被停摆）：
  //  - 从未 orderFront（WKWebView 冻结，rAF 不执行）→ false：直接 invoke 解冻，
  //    首帧无旧 bitmap 不会闪现。
  //  - show-then-hide（hide 不 orderOut，alpha=0 仍 ordered-in）→ true：两层 rAF
  //    等 Vue 视图切换 + paint 完成再 show，避免 orderFront 合成上一帧旧视图闪现。
  if (
    await getCurrentWindow()
      .isVisible()
      .catch(() => false)
  ) {
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        invoke(CMD.showWindow).catch(() => {})
      })
    })
  } else {
    invoke(CMD.showWindow).catch(() => {})
  }
}

export const isTauri =
  typeof window !== 'undefined' &&
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ !== undefined
