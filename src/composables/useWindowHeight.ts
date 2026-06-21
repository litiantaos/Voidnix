import { watch, type ComputedRef } from 'vue'
import { getCurrentWindow, LogicalSize } from '@tauri-apps/api/window'
import { WINDOW } from '@/runtime/constants'
import { isTauri } from '@/utils/tauri'
import type { Extension } from '@/runtime/types'

function clampHeight(h: number): number {
  return Math.max(WINDOW.MIN_HEIGHT, Math.min(WINDOW.MAX_HEIGHT, h))
}

/// 模块激活时按扩展声明的 windowHeight 调整主窗口高度；退出恢复默认。
/// 仅高度可调（宽度固定 WINDOW.WIDTH）；subview 跟随所在 module 声明值。
export function useWindowHeight(activeModule: ComputedRef<Extension | null>) {
  if (!isTauri) return
  const win = getCurrentWindow()
  watch(activeModule, (mod) => {
    const h = mod?.windowHeight ? clampHeight(mod.windowHeight) : WINDOW.DEFAULT_HEIGHT
    win.setSize(new LogicalSize(WINDOW.WIDTH, h)).catch(() => {})
  })
}
