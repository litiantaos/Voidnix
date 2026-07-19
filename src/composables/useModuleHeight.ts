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
/// auto 模式：ResizeObserver 监听内容根（contentRef）实际高度，窗口高 = CHROME_HEIGHT + 内容高，
/// clamp [DEFAULT_HEIGHT, 屏幕高 90%]，底部将出屏（含间距）则上移，离开 auto 还原进入前位置。
///
/// 用法（MainView 全局唯一调用）：
///   useModuleHeight({ activeModule, activeSubview, contentRef })
export function useModuleHeight(deps: {
  activeModule: ComputedRef<Extension | null>
  activeSubview: ComputedRef<string | null>
  contentRef: Ref<HTMLElement | undefined>
}) {
  if (!isTauri) return

  const { activeModule, activeSubview, contentRef } = deps
  const tauriWindow = getCurrentWindow()
  let ro: ResizeObserver | null = null
  // 逻辑目标位置（上次 setMainFrame 设定值）。animator 动画期间 outerPosition 返回
  // 动画中间瞬时值，连续 adjust 若以此为准会导致位置/尺寸漂移，故以逻辑目标为准。
  // 首次未初始化时才读一次实际窗口位置。
  let targetX: number | null = null
  let targetY: number | null = null
  // auto 进入前的稳定逻辑 Y，离开 auto 时还原
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
    // 当前屏全局 Y 范围（多屏下用窗口所在屏，而非 mainScreen / 单屏高）
    const screenTop = monitor.position.y / factor
    const screenBottom = (monitor.position.y + monitor.size.height) / factor
    // 基准位置：优先逻辑目标（动画中读 outerPosition 得中间值会漂移），首次读实际位置。
    // show 时 Rust 会把窗移到光标屏，与缓存逻辑坐标可差数百 px —— 偏差过大则以实际为准。
    const pos = await tauriWindow.outerPosition()
    const actualX = pos.x / factor
    const actualY = pos.y / factor
    let baseX: number
    let baseY: number
    if (
      targetX !== null &&
      targetY !== null &&
      Math.abs(actualX - targetX) < 80 &&
      Math.abs(actualY - targetY) < 80
    ) {
      baseX = targetX
      baseY = targetY
    } else {
      baseX = actualX
      baseY = actualY
      targetX = baseX
      targetY = baseY
      // 跨屏 reposition 后 auto 原位失效，避免回到旧屏
      originalY = null
      wasAuto = false
    }

    // ── 计算目标高度 ──
    let target: number
    if (mode.mode === 'fixed') {
      target = clampHeight(mode.value)
    } else if (mode.mode === 'auto') {
      const ct = contentRef.value
      if (!ct) return
      const contentH = ct.offsetHeight
      const maxH = Math.round((screenBottom - screenTop) * 0.9)
      target = Math.max(WINDOW.DEFAULT_HEIGHT, Math.min(WINDOW.CHROME_HEIGHT + contentH, maxH))
    } else {
      target = WINDOW.DEFAULT_HEIGHT
    }

    // ── 计算目标 Y（上移策略）──
    if (mode.mode === 'auto' && !wasAuto) {
      originalY = baseY
      wasAuto = true
    }
    let nextY = baseY
    if (mode.mode === 'auto') {
      // 底部将出屏（含间距）则上移，clamp 不超过当前屏顶
      if (nextY + target + BOTTOM_MARGIN > screenBottom) {
        nextY = Math.max(screenTop, screenBottom - target - BOTTOM_MARGIN)
      }
    } else if (wasAuto && originalY !== null) {
      // 离开 auto：还原进入前位置
      nextY = originalY
      originalY = null
      wasAuto = false
    }

    targetY = nextY

    // 一次 IPC 触发系统 animator 动画（CoreAnimation 接管，非 JS 逐帧）
    invoke(CMD.setMainFrame, {
      x: baseX,
      y: nextY,
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

  // show 时 Rust 已 center_on_cursor_screen；聚焦后丢弃逻辑坐标缓存，避免 setMainFrame
  // 用上一屏的 target 把窗拉回主屏（副屏「只有首次能出来」的根因）。
  let unlistenFocus: (() => void) | null = null

  onMounted(() => {
    nextTick(() => {
      syncObserver()
      adjust()
    })
    void tauriWindow
      .onFocusChanged(({ payload: focused }) => {
        if (!focused) return
        targetX = null
        targetY = null
        originalY = null
        wasAuto = false
        nextTick(() => adjust())
      })
      .then((un) => {
        unlistenFocus = un
      })
      .catch(() => {})
  })

  onBeforeUnmount(() => {
    ro?.disconnect()
    ro = null
    unlistenFocus?.()
    unlistenFocus = null
  })
}
