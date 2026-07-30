import { watch, nextTick, onMounted, onBeforeUnmount, type ComputedRef, type Ref } from 'vue'
import { getCurrentWindow, currentMonitor, type Monitor } from '@tauri-apps/api/window'
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
///   useExtensionHeight({ activeExtension, activeSubview, contentRef })
export function useExtensionHeight(deps: {
  activeExtension: ComputedRef<Extension | null>
  activeSubview: ComputedRef<string | null>
  contentRef: Ref<HTMLElement | undefined>
}) {
  if (!isTauri) return

  const { activeExtension, activeSubview, contentRef } = deps
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
  // monitor 缓存：屏幕信息很少变化，仅在 focus 变化（跨屏 show）时失效
  interface MonitorBounds {
    factor: number
    screenTop: number
    screenBottom: number
  }
  let cachedBounds: MonitorBounds | null = null
  // rAF 合帧标志：一帧内多次 RO 回调只触发一次 adjust
  let rafQueued = false
  let rafId: number | null = null
  // 上次实际下发的 frame：目标无变化时跳过 invoke，防止 ResizeObserver ↔ animate_frame 正反馈死循环
  // （动画期间 content reflow 触发 RO → adjust → 新动画 → 再 reflow → …，目标高度 ±1px 抖动即自维持）
  let lastApplied: { h: number; y: number } | null = null

  function invalidateMonitor() {
    cachedBounds = null
  }

  async function getBounds(): Promise<MonitorBounds | null> {
    if (cachedBounds) return cachedBounds
    const monitor: Monitor | null = await currentMonitor().catch(() => null)
    if (!monitor) return null
    const factor = monitor.scaleFactor
    cachedBounds = {
      factor,
      screenTop: monitor.position.y / factor,
      screenBottom: (monitor.position.y + monitor.size.height) / factor,
    }
    return cachedBounds
  }

  /// rAF 合帧：RO 多次触发合并为单次 adjust，避免 auto 模式流式搜索时 IPC 风暴
  function scheduleAdjust() {
    if (rafQueued) return
    rafQueued = true
    rafId = requestAnimationFrame(() => {
      rafQueued = false
      rafId = null
      void adjust()
    })
  }

  function currentMode(): HeightMode {
    const ext = activeExtension.value
    if (!ext) return { mode: 'default' }
    const subId = activeSubview.value
    if (subId && ext.subviewHeights?.[subId] !== undefined) {
      const v = ext.subviewHeights[subId]
      return v === 'auto' ? { mode: 'auto' } : { mode: 'fixed', value: v }
    }
    if (ext.windowHeight === 'auto') return { mode: 'auto' }
    if (typeof ext.windowHeight === 'number') return { mode: 'fixed', value: ext.windowHeight }
    return { mode: 'default' }
  }

  async function adjust() {
    const mode = currentMode()
    const bounds = await getBounds()
    if (!bounds) return
    const { factor, screenTop, screenBottom } = bounds
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

    // 目标无变化（≤1px）则跳过：animate_frame 每次 invoke 启动 0.26s 动画，
    // 动画期间 content reflow 可能触发 ResizeObserver → 再 adjust → 再动画，形成死循环。
    // 跳过等价于"窗口已在正确位置"，不启动新动画，content 稳定后 RO 自然停止。
    if (
      lastApplied &&
      Math.abs(target - lastApplied.h) <= 1 &&
      Math.abs(nextY - lastApplied.y) <= 1
    ) {
      return
    }
    lastApplied = { h: target, y: nextY }

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
    ro = new ResizeObserver(() => scheduleAdjust())
    ro.observe(ct)
  }

  // 扩展 / subview 切换：同步 observer + 重算（系统 animator 自动从中断点接续）
  watch([activeExtension, activeSubview], () => {
    lastApplied = null
    nextTick(() => {
      syncObserver()
      adjust()
    })
  })

  watch(contentRef, (el) => {
    if (el && currentMode().mode === 'auto' && !ro) {
      ro = new ResizeObserver(() => scheduleAdjust())
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
        lastApplied = null
        invalidateMonitor()
        nextTick(() => adjust())
      })
      .then((un) => {
        unlistenFocus = un
      })
      .catch(() => {})
  })

  onBeforeUnmount(() => {
    if (rafId !== null) cancelAnimationFrame(rafId)
    ro?.disconnect()
    ro = null
    unlistenFocus?.()
    unlistenFocus = null
  })
}
