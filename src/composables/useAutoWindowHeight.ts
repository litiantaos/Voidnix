import { onMounted, onBeforeUnmount, onActivated, onDeactivated, nextTick, type Ref } from 'vue'
import {
  getCurrentWindow,
  LogicalSize,
  LogicalPosition,
  currentMonitor,
} from '@tauri-apps/api/window'
import { WINDOW } from '@/runtime/constants'
import { isTauri } from '@/utils/tauri'
import { resolveModuleHeight } from '@/composables/useWindowHeight'
import { getExtension } from '@/runtime/extension-registry'
import { useAppStore } from '@/stores/app'

const BOTTOM_MARGIN = 40 // 窗口离屏幕底部间距（逻辑 px），留足避免压 Dock
const ANIM_DURATION = 180 // 高度/位置过渡时长（ms）

/// 主窗口高度随子视图内容自适应。与 useWindowHeight（模块声明固定 windowHeight）互补：
/// 后者管静态模块，本 hook 管「内容高度可变」的子视图（OCR / 翻译结果等）。
///
/// 双层结构：root（h-full 撑满父，量 chrome 固定开销）+ content（自然高，量真实内容高）。
/// ResizeObserver 监听 content 变化 → 窗口高 = chrome + 内容高，clamp [MIN_HEIGHT, 屏幕高 90%]；
/// 高度变化以 rAF 插值动画过渡（Tauri setSize 瞬时，需自行补间），底部将出屏则同步上移并留间距。
/// 退出还原 resolveModuleHeight（与 useWindowHeight 单一真相）。
///
/// 用法：调用方声明两个 ref 绑定到双层 div 后传入：
///   <div ref="rootRef" h-full overflow-y-auto><div ref="contentRef"> …内容… </div></div>
///   const rootRef = ref<HTMLElement>(); const contentRef = ref<HTMLElement>()
///   useAutoWindowHeight({ rootRef, contentRef })
export function useAutoWindowHeight(refs: {
  rootRef: Ref<HTMLElement | undefined>
  contentRef: Ref<HTMLElement | undefined>
}) {
  if (!isTauri) return

  const { rootRef, contentRef } = refs
  const tauriWindow = getCurrentWindow()
  let ro: ResizeObserver | null = null
  // chrome = 搜索栏 + 状态栏等固定开销（常量）。仅在 root 正确撑满（内容未溢出）时测量可信：
  // 内容小、root 必撑满，首测缓存真值后复用，规避内容撑开后 clientHeight 失真（曾导致 chrome 为负）。
  let chromeH = 0
  // 跟踪当前窗口实际高度/位置（逻辑 px），作动画起点；null 表示待从系统读取（跨 activate 重读）
  let curH: number = WINDOW.DEFAULT_HEIGHT
  let curX = 0
  let curY: number | null = null
  // 进入 autoHeight 时的原始窗口 y（上移前），退出时还原位置避免上移后位置残留
  let originalY: number | null = null
  let animId: number | null = null

  async function syncFrame(factor: number) {
    const pos = await tauriWindow.outerPosition()
    const size = await tauriWindow.outerSize()
    curX = pos.x / factor
    curY = pos.y / factor
    curH = size.height / factor
  }

  function animate(toH: number, toY: number) {
    if (animId !== null) cancelAnimationFrame(animId)
    const fromH = curH
    const fromY = curY ?? toY
    const start = performance.now()
    const step = (now: number) => {
      const t = Math.min(1, (now - start) / ANIM_DURATION)
      const e = 1 - Math.pow(1 - t, 3) // easeOutCubic
      curH = fromH + (toH - fromH) * e
      curY = fromY + (toY - fromY) * e
      tauriWindow.setPosition(new LogicalPosition(curX, Math.round(curY))).catch(() => {})
      tauriWindow.setSize(new LogicalSize(WINDOW.WIDTH, Math.round(curH))).catch(() => {})
      if (t < 1) {
        animId = requestAnimationFrame(step)
      } else {
        curH = toH
        curY = toY
        animId = null
      }
    }
    animId = requestAnimationFrame(step)
  }

  async function adjust() {
    const root = rootRef.value
    const content = contentRef.value
    if (!root || !content) return
    const monitor = await currentMonitor().catch(() => null)
    if (!monitor) return
    const factor = monitor.scaleFactor
    const screenH = monitor.size.height / factor
    if (curY === null) {
      await syncFrame(factor)
      originalY = curY
    }
    if (chromeH === 0 && root.clientHeight >= content.offsetHeight) {
      chromeH = window.innerHeight - root.clientHeight
    }
    const chrome = chromeH || window.innerHeight - root.clientHeight
    const contentH = content.offsetHeight
    const maxH = Math.round(screenH * 0.9)
    // 下限 DEFAULT_HEIGHT：内容变少（loading / 清空）时不缩到比默认窗口矮，与退出还原值一致
    const target = Math.max(WINDOW.DEFAULT_HEIGHT, Math.min(chrome + contentH, maxH))
    // 默认 top 不变；底部将出屏（含间距）则上移，保证完整可见且不压 Dock
    let targetY = curY ?? 0
    if (targetY + target + BOTTOM_MARGIN > screenH) {
      targetY = Math.max(0, screenH - target - BOTTOM_MARGIN)
    }
    animate(target, targetY)
  }

  onMounted(() => {
    if (!contentRef.value) return
    ro = new ResizeObserver(() => adjust())
    ro.observe(contentRef.value)
  })
  onActivated(() => {
    curY = null // 跨 activate 重读实际位置/高度，避免上次动画/外部改动遗留失准
    nextTick(adjust)
  })
  // 退出 autoHeight 视图：恢复所属模块的目标高度（声明 windowHeight 则用之，否则 DEFAULT）。
  // 复用 resolveModuleHeight 保证与 useWindowHeight 单一真相：subview 退出回 mainView 时高度一致，
  // mainView 切走时虽被新模块 watch 覆盖，但恢复正确值避免时序竞争留下错值。
  onDeactivated(() => {
    if (animId !== null) cancelAnimationFrame(animId)
    const mod = getExtension(useAppStore().activeModuleId ?? '') ?? null
    tauriWindow.setSize(new LogicalSize(WINDOW.WIDTH, resolveModuleHeight(mod))).catch(() => {})
    // 退出还原进入前位置，避免内容上移后位置残留
    if (originalY !== null && curY !== null && curY !== originalY) {
      tauriWindow.setPosition(new LogicalPosition(curX, originalY)).catch(() => {})
      curY = originalY
    }
  })
  onBeforeUnmount(() => {
    if (animId !== null) cancelAnimationFrame(animId)
    ro?.disconnect()
    ro = null
  })
}
