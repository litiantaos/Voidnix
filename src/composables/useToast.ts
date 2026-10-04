import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { isTauri } from '@/utils/tauri'

export type ToastKind = 'success' | 'error'

export interface ToastOptions {
  duration?: number
  kind?: ToastKind
}

/** 原生 toast 薄壳：Rust 端独立 NSPanel 浮窗（主窗隐藏不影响展示），恒锚主窗
 *  右下外侧；无 hover 交互、无页面内状态。返回 invoke promise（已吞错），供
 *  toastAndHide 顺序化（先 toast 后 hide，防 IPC 并发乱序）。 */
export function showToast(message: string, opts?: ToastOptions): Promise<void> {
  if (!isTauri) return Promise.resolve()
  return invoke(CMD.showToast, {
    message,
    kind: opts?.kind ?? 'success',
    durationMs: opts?.duration ?? 2000,
  })
    .catch(() => {})
    .then(() => {})
}
