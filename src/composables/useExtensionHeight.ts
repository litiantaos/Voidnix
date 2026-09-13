import { watch, nextTick, onMounted, onBeforeUnmount, type ComputedRef, type Ref } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { WINDOW } from '@/runtime/constants'
import { isTauri } from '@/utils/tauri'
import type { Extension } from '@/runtime/types'

const BOTTOM_MARGIN = 40 // 窗口离屏幕底部间距（逻辑 px），留足避免压 Dock
/// set_main_frame 后该窗口期内的窗口位移视为自身 animator 动画中间态（动画 0.26s + 余量），
/// 不触发逻辑坐标缓存失效
const ANIM_SETTLE_MS = 400

function clampHeight(h: number): number {
  return Math.max(WINDOW.MIN_HEIGHT, Math.min(WINDOW.MAX_HEIGHT, h))
}

type HeightMode = { mode: 'fixed'; value: number } | { mode: 'auto' } | { mode: 'default' }

/// get_main_frame 返回（Cocoa 逻辑坐标，y 向上、y 为窗口底边）。
/// 位置真值必须走本命令而非 Tauri outerPosition/currentMonitor：后者经 tao 转成
/// top-left（y 向下、参照主屏物理高）语义，与 set_main_frame 的 NSWindow setFrame
/// （Cocoa）坐标系相反，混用会致位置错乱。
interface MainFrameInfo {
  x: number
  y: number
  width: number
  height: number
  /** visibleFrame 底边 y（数值小的一方，扣菜单栏/Dock） */
  visBottomY: number
  /** visibleFrame 顶边 y（数值大的一方） */
  visTopY: number
  /** auto 高度天花板（visibleFrame × 0.9，与 animate_frame clamp 同源） */
  maxHeight: number
}

/// 统一的主窗口高度管理。扩展声明 windowHeight（number 固定 / 'auto' 自适应 / 未声明默认），
/// 框架统一处理。高度过渡交给 macOS 系统 animator（NSAnimationContext + setFrame:display:animate:），
/// CoreAnimation 接管插值，不逐帧阻塞主线程、不逐帧触发 WebView 重排，流畅度远超 JS rAF。
///
/// 位置（x/y）以前端为单一真相源：顶边锚定模型——fixed/default 保顶边（高度变化视觉连续）、
/// auto 增高底边将出屏则上移顶边、离开 auto 还原进入前顶边、用户拖动后以实际位置重锚。
/// Rust animate_frame 只做屏内 clamp 与严重出屏复位兜底，不自行计算位置。
///
/// auto 模式：ResizeObserver 监听内容根（contentRef）实际高度，窗口高 = CHROME_HEIGHT + 内容高，
/// clamp [DEFAULT_HEIGHT, Rust 同源天花板]，底部将出屏（含间距）则上移，离开 auto 还原进入前顶边。
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
  // 逻辑目标位置（上次 setMainFrame 设定值）。animator 动画期间 frame 读取返回
  // 动画中间瞬时值，连续 adjust 若以此为准会导致位置/尺寸漂移，故以逻辑目标为准。
  // 首次未初始化时才读一次实际窗口位置。锚定顶边（Cocoa y+h）：高度变化下顶边才是稳定锚。
  let targetTop: number | null = null
  let targetX: number | null = null
  // auto 进入前的稳定逻辑顶边，离开 auto 时还原
  let originalTop: number | null = null
  let wasAuto = false
  // rAF 合帧标志：一帧内多次 RO 回调只触发一次 adjust
  let rafQueued = false
  let rafId: number | null = null
  // 上次实际下发的 frame：目标无变化时跳过 invoke，防止 ResizeObserver ↔ animate_frame 正反馈死循环
  // （动画期间 content reflow 触发 RO → adjust → 新动画 → 再 reflow → …，目标高度 ±1px 抖动即自维持）
  let lastApplied: { h: number; y: number } | null = null
  // 上次 set_main_frame 下发时刻：其后 ANIM_SETTLE_MS 内的 onMoved 是自身动画中间态
  let lastFrameSetAt = 0
  // 窗口可见性（focus/blur 驱动）：不可见时 adjust 跳过 set_main_frame，避免提前改高度
  // 导致 present 后 WKWebView viewport 与 NSWindow frame 不匹配（footer 定位错位）。
  // 初始 false：窗口启动 visible:false（tauri.conf），首次 focus 前不可见。
  // focus 事件序列不可靠（show 过程瞬时页面 blur 会打掉置位、二次补发迟至数秒甚至丢失），
  // 被拦截时经 Rust 权威可见性（show/hide 维护的 WINDOW_VISIBLE）复核自愈。
  let windowVisible = false

  /// 一次 IPC 取当前 frame（每次都需最新，动画中为中间值）+ 屏几何（visibleFrame，
  /// 随 Rust placement 锁定实时更新，跨屏 show 后即为新屏值）。
  async function getFrameInfo(): Promise<MainFrameInfo | null> {
    return invoke<MainFrameInfo | null>(CMD.getMainFrame).catch(() => null)
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
    // 窗口不可见时跳过 set_main_frame：adjust 会把 NSWindow frame 提前改成目标高度，
    // 但 WKWebView viewport 仍停留在旧高度——present 以新高度 visible 后，footer（absolute
    // 定位）在旧 viewport 底部，离窗口实际底部很远，表现为「输入框悬在中间」。
    // 跳过后 present 用上次稳定高度（viewport 匹配），show 后 focus 触发 adjust，
    // 渐进 animate 到目标高度，WKWebView viewport 逐帧跟随、footer 始终贴底。
    // 但 focus 事件序列会被 show 过程的瞬时页面 blur 打乱（windowVisible 假阴性而窗口
    // 实际可见），fixed/default 模式无 RO、切换后无任何机制再触发 adjust——高度与位置
    // 将停留在上个扩展的设定值（「切到默认高度界面回不去」的根因）。经 Rust 权威
    // 可见性复核自愈：真可见则继续，真隐藏照旧跳过。
    if (!windowVisible) {
      const visible = await invoke<boolean>(CMD.isMainWindowVisible).catch(() => false)
      if (!visible) return
      windowVisible = true
    }
    const mode = currentMode()
    const info = await getFrameInfo()
    if (!info) return
    const { visBottomY, visTopY, maxHeight } = info
    // 基准位置：优先逻辑目标（动画中读实际值得中间值会漂移），首次读实际位置。
    // show 时 Rust 会把窗移到光标屏，与缓存逻辑坐标可差数百 px —— 偏差过大则以实际为准。
    // 同一读取顺带取得实际高度：fixed/default 已等于目标则跳过（读实际高度兜底）。
    const actualX = info.x
    const actualTop = info.y + info.height
    const actualH = info.height
    let baseX: number
    let baseTop: number
    if (
      targetX !== null &&
      targetTop !== null &&
      Math.abs(actualX - targetX) < 80 &&
      Math.abs(actualTop - targetTop) < 80
    ) {
      baseX = targetX
      baseTop = targetTop
    } else {
      baseX = actualX
      baseTop = actualTop
      targetX = baseX
      targetTop = baseTop
      // 跨屏 reposition / 用户拖动等外部位移后 auto 还原锚失效，避免跳回旧位置
      originalTop = null
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
      target = Math.max(WINDOW.DEFAULT_HEIGHT, Math.min(WINDOW.CHROME_HEIGHT + contentH, maxHeight))
    } else {
      target = WINDOW.DEFAULT_HEIGHT
    }

    // ── 计算目标顶边（Cocoa y 向上，顶边 = y + h，数值大 = 更靠上）──
    if (mode.mode === 'auto' && !wasAuto) {
      originalTop = baseTop
      wasAuto = true
    }
    let nextTop = baseTop
    if (mode.mode === 'auto') {
      // 保顶边向下生长；底边将出屏（含间距）则上移顶边（底边锚定 visBottomY+40），
      // 顶边不超 visible 顶（屏太矮装不下时底边出屏，Rust clamp 兜底拉正）
      if (nextTop - target < visBottomY + BOTTOM_MARGIN) {
        nextTop = Math.min(visBottomY + BOTTOM_MARGIN + target, visTopY)
      }
    } else if (wasAuto && originalTop !== null) {
      // 离开 auto：还原进入前顶边
      nextTop = originalTop
      originalTop = null
      wasAuto = false
    }

    targetTop = nextTop
    const nextY = nextTop - target

    // fixed/default：窗口实际高度已达目标且顶边无需移动则跳过。已可见时连续切相同高度的扩展
    // （如 agent 820 → proxy 820）命中。即便 from==to，animator setFrame 仍启动 animation
    // context 触发 WKWebView reflow；读实际高度兜底跳过，回填 lastApplied。
    // auto 上移过的顶边也需还原：离开 auto 时 nextTop=originalTop≠baseTop 不能因高度相等就跳过。
    if (
      mode.mode !== 'auto' &&
      Math.abs(target - actualH) <= 1 &&
      Math.abs(nextTop - baseTop) <= 1
    ) {
      lastApplied = { h: target, y: nextY }
      return
    }

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
    lastFrameSetAt = performance.now()
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
  let unlistenMoved: (() => void) | null = null

  onMounted(() => {
    nextTick(() => {
      syncObserver()
      adjust()
    })
    void tauriWindow
      .onFocusChanged(({ payload: focused }) => {
        if (!focused) {
          windowVisible = false
          return
        }
        windowVisible = true
        targetX = null
        targetTop = null
        originalTop = null
        wasAuto = false
        lastApplied = null
        nextTick(() => adjust())
      })
      .then((un) => {
        unlistenFocus = un
      })
      .catch(() => {})
    // 外部位移（用户拖动）重锚：丢弃逻辑坐标缓存，下次 adjust 经 get_main_frame 以实际
    // 位置为基准（否则小幅拖动落在 <80px 容差内会被缓存拽回原位）。payload 是 tao
    // top-left 物理坐标，不消费数值只消费事件本身。自身 animator 动画（set_main_frame
    // 后 ANIM_SETTLE_MS 内）的中间态位移不算外部位移。
    void tauriWindow
      .onMoved(() => {
        if (performance.now() - lastFrameSetAt < ANIM_SETTLE_MS) return
        targetX = null
        targetTop = null
      })
      .then((un) => {
        unlistenMoved = un
      })
      .catch(() => {})
  })

  onBeforeUnmount(() => {
    if (rafId !== null) cancelAnimationFrame(rafId)
    ro?.disconnect()
    ro = null
    unlistenFocus?.()
    unlistenFocus = null
    unlistenMoved?.()
    unlistenMoved = null
  })
}
