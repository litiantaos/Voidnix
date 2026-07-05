import { watch, nextTick, onMounted, onBeforeUnmount, type ComputedRef, type Ref } from 'vue'
import { getCurrentWindow, currentMonitor } from '@tauri-apps/api/window'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { WINDOW } from '@/runtime/constants'
import { isTauri } from '@/utils/tauri'
import type { Extension } from '@/runtime/types'

const BOTTOM_MARGIN = 40 // 窗口离屏幕底部间距（逻辑 px），留足避免压 Dock

function clampHeight(h: number): number {
  return Math.max(WINDOW.MIN_HEIGHT, Math.min(WINDOW.MAX_HEIGHT, h))
}

type HeightMode = { mode: 'fixed'; value: number } | { mode: 'auto' } | { mode: 'default' }

/// 统一的主窗口高度管理。扩展声明 windowHeight（number 固定 / 'auto' 自适应 / 未声明默认），
/// 框架统一处理。高度过渡交给 macOS 系统 animator（NSAnimationContext + setFrame:display:animate:），
/// CoreAnimation 接管插值，不逐帧阻塞主线程、不逐帧触发 WebView 重排，流畅度远超 JS rAF。
///
/// auto 模式：ResizeObserver 监听内容根（contentRef）实际高度，窗口高 = chrome + 内容高，
/// clamp [DEFAULT_HEIGHT, 屏幕高 90%]，底部将出屏（含间距）则上移，离开 auto 还原进入前位置。
///
/// 用法（MainView 全局唯一调用）：
///   useModuleHeight({ activeModule, activeSubview, searchBarRef, contentRef })
export function useModuleHeight(deps: {
  activeModule: ComputedRef<Extension | null>
  activeSubview: ComputedRef<string | null>
  searchBarRef: Ref<HTMLElement | undefined>
  contentRef: Ref<HTMLElement | undefined>
}) {
  if (!isTauri) return

  const { activeModule, activeSubview, searchBarRef, contentRef } = deps
  const tauriWindow = getCurrentWindow()
  let ro: ResizeObserver | null = null
  let chromeH = 0
  // 当前窗口 x + 上移策略所需的 originalY（auto 进入前位置，离开 auto 还原）
  let curX = 0
  let originalY: number | null = null
  let wasAuto = false

  function currentMode(): HeightMode {
    const mod = activeModule.value
    if (!mod) return { mode: 'default' }
    const subId = activeSubview.value
    if (subId && mod.subviewHeights?.[subId] !== undefined) {
      const v = mod.subviewHeights[subId]
      return v === 'auto' ? { mode: 'auto' } : { mode: 'fixed', value: v }
    }
    if (mod.windowHeight === 'auto') return { mode: 'auto' }
    if (typeof mod.windowHeight === 'number') return { mode: 'fixed', value: mod.windowHeight }
    return { mode: 'default' }
  }

  async function adjust() {
    const mode = currentMode()
    const monitor = await currentMonitor().catch(() => null)
    if (!monitor) return
    const factor = monitor.scaleFactor
    const screenH = monitor.size.height / factor
    // 读当前位置（系统 animator 异步，每次 adjust 重读避免上次动画遗留）
    const pos = await tauriWindow.outerPosition()
    curX = pos.x / factor
    const curY = pos.y / factor

    // ── 计算目标高度 ──
    let target: number
    if (mode.mode === 'fixed') {
      target = clampHeight(mode.value)
    } else if (mode.mode === 'auto') {
      const sb = searchBarRef.value
      const ct = contentRef.value
      if (!sb || !ct) return
      if (chromeH === 0) chromeH = sb.offsetHeight
      const contentH = ct.offsetHeight
      const maxH = Math.round(screenH * 0.9)
      target = Math.max(WINDOW.DEFAULT_HEIGHT, Math.min(chromeH + contentH, maxH))
    } else {
      target = WINDOW.DEFAULT_HEIGHT
    }

    // ── 计算目标 Y（上移策略）──
    if (mode.mode === 'auto' && !wasAuto) {
      originalY = curY
      wasAuto = true
    }
    let targetY = curY
    if (mode.mode === 'auto') {
      if (targetY + target + BOTTOM_MARGIN > screenH) {
        targetY = Math.max(0, screenH - target - BOTTOM_MARGIN)
      }
    } else if (wasAuto && originalY !== null) {
      // 离开 auto：还原进入前位置
      targetY = originalY
      originalY = null
      wasAuto = false
    }

    // 一次 IPC 触发系统 animator 动画（CoreAnimation 接管，非 JS 逐帧）
    invoke(CMD.setMainFrame, {
      x: curX,
      y: targetY,
      width: WINDOW.WIDTH,
      height: target,
    }).catch(() => {})
  }

  function syncObserver() {
    ro?.disconnect()
    ro = null
    if (currentMode().mode !== 'auto') return
    const ct = contentRef.value
    if (!ct) return
    ro = new ResizeObserver(() => adjust())
    ro.observe(ct)
  }

  // 模块 / subview 切换：同步 observer + 重算（系统 animator 自动从中断点接续）
  watch([activeModule, activeSubview], () => {
    nextTick(() => {
      syncObserver()
      adjust()
    })
  })

  watch(contentRef, (el) => {
    if (el && currentMode().mode === 'auto' && !ro) {
      ro = new ResizeObserver(() => adjust())
      ro.observe(el)
    }
  })

  onMounted(() => {
    nextTick(() => {
      syncObserver()
      adjust()
    })
  })

  onBeforeUnmount(() => {
    ro?.disconnect()
    ro = null
  })
}
