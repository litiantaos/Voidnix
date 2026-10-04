import { defineStore } from 'pinia'
import { markRaw, ref, type Component } from 'vue'
import { searchEngine } from '@/runtime/search-engine'
import { t } from '@/runtime/i18n'
import { hideWindow, showWindow } from '@/utils/tauri'
import { writeText } from '@/utils/clipboard'
import { showToast, type ToastOptions } from '@/composables/useToast'

export interface ConfirmOptions {
  title: string
  message?: string
  /** message 以 markdown 渲染（BaseDialog markdown 模式） */
  markdown?: boolean
  size?: 'sm' | 'md' | 'lg'
  okLabel?: string
  cancelLabel?: string
  showCancel?: boolean
}

/// WebContent 内存阈值触发的 navigate 重载会清零全部 JS 单例；sessionStorage 在同一
/// 浏览会话内跨导航存活——激活扩展写入此处，重载后据此恢复隐藏前视图。
/// 应用冷启动是全新会话（sessionStorage 为空），不恢复。
const ACTIVE_EXT_KEY = 'voidnix.active-ext'

function persistActiveExt(id: string | null) {
  try {
    if (id) sessionStorage.setItem(ACTIVE_EXT_KEY, id)
    else sessionStorage.removeItem(ACTIVE_EXT_KEY)
  } catch {
    /* 存储不可用仅损失重载恢复，静默降级 */
  }
}

function restoreActiveExt(): string | null {
  try {
    return sessionStorage.getItem(ACTIVE_EXT_KEY)
  } catch {
    return null
  }
}

export const useAppStore = defineStore('app', () => {
  const restoredExtId = restoreActiveExt()
  const activeExtId = ref<string | null>(restoredExtId)
  const searchQuery = ref('')
  // 进入扩展时的入口 query 快照：ESC 退出据此决定返回目标（/ → 工具列表，其余 → 主界面）
  const entryQuery = ref('')
  const isComposing = ref(false)

  const isDialogOpen = ref(false)
  const dialogOptions = ref<ConfirmOptions | null>(null)
  const lastDialogCloseTime = ref(0)
  // 原生系统对话框（如文件选择器）打开期间，抑制失焦隐藏（窗口重获焦点时自动解除）
  const suppressBlur = ref(false)
  let dialogResolve: ((value: boolean) => void) | null = null

  const activeSubview = ref<string | null>(null)
  // 子视图是否经由外部事件（open-extension-subview）打开：外部打开时 ESC 直接回主界面，
  // 内部打开（从扩展 mainView 进入 config 等）ESC 返回 mainView。
  const subviewExternal = ref(false)
  const shortcutRecording = ref(false)

  /// 整窗视图（框架级）：非 null 时接管整个主窗口（搜索栏 + 内容区让位，MainView 统一渲染）。
  /// 视图组件以 done 事件请求完结；激活/退出策略由供给方（如首启引导）经 watch 驱动。
  const fullscreenView = ref<Component | null>(null)

  /// 整窗视图完结时恢复的扩展 id：供给方写入（如设置页重看引导），框架 done 处理消费并清空；
  /// 槽被其它路径清空（扩展激活让位）时同样清空防过期
  const fullscreenReturnExtId = ref<string | null>(null)

  /// 扩展间导航的 Esc 返回目标：setActiveExtension(id, from) 带来源时记忆（如 ai-gateway →
  /// ai-providers），exitExtension 消费；其余激活路径一律清空防过期——返回跳转本身不带
  /// from，故返回后再 Esc 走原 entryQuery 逻辑，链自然收敛回主界面
  const extensionReturnExtId = ref<string | null>(null)

  /// 自测模式一次性标志（main.ts 查询 is_self_test_mode 后置位）：接管式 UI（首启引导等）
  /// 据此让位——整窗接管会藏起搜索输入，CGEvent 打字进不去；窗口由测试脚本驱动。
  const selfTestMode = ref(false)

  const shortcutErrors = ref<Record<string, string>>({})

  /** 返回 invoke promise（已吞错）：toastAndHide 顺序化依赖（先 toast 入队再隐藏） */
  function showStatus(msg: string, opts?: ToastOptions) {
    return showToast(msg, opts)
  }

  function setActiveExtension(id: string | null, from?: string | null) {
    const prevId = activeExtId.value
    if (id && !prevId) {
      // 从外部进入扩展：快照入口 query（统一所有激活路径——快捷键 toggle / open-extension
      // 事件 / 扩展自激活等均经此，避免 useSearchInput 旁路漏存）
      entryQuery.value = searchQuery.value
    } else if (!id && prevId) {
      // 退出扩展：清空
      entryQuery.value = ''
    }
    // 全局 confirm（App.vue Teleport，不在 KeepAlive 内）：切扩展时按取消收束，避免遮罩残留
    if (isDialogOpen.value && id !== prevId) {
      resolveConfirm(false)
    }
    // ext→ext（如 OCR→translate）：保留原入口；带 from 的导航另有 Esc 返回目标
    // （extensionReturnExtId），返回链收敛后仍由入口逻辑退出
    activeExtId.value = id
    // 扩展间导航（带 from）：记忆 Esc 返回目标；其余激活路径一律清空防过期
    extensionReturnExtId.value = id && from && from !== id ? from : null
    persistActiveExt(id)
    activeSubview.value = null
    subviewExternal.value = false
    // 模式切换：激活扩展时 searchEngine 只调该扩展 dynamic；null 恢复全局聚合。
    // 扩展 onActivate/onDeactivate 由各 View 的 onActivated/onDeactivated（KeepAlive）承接。
    // 扩展内 BaseDialog 由自身 onDeactivated(dismiss) 关窗。
    searchEngine.setActiveExtension(id ?? undefined)
  }

  // navigate 重载后恢复：store 创建时若已有持久化激活扩展（重载前活跃），
  // 搜索引擎同步进扩展模式，首个 show 帧即回到隐藏前视图
  if (restoredExtId) searchEngine.setActiveExtension(restoredExtId)

  function setSearchQuery(query: string) {
    searchQuery.value = query
  }

  function setComposing(status: boolean) {
    isComposing.value = status
  }

  function showConfirm(options: ConfirmOptions): Promise<boolean> {
    dialogOptions.value = options
    isDialogOpen.value = true
    return new Promise((resolve) => {
      dialogResolve = resolve
    })
  }

  function resolveConfirm(result: boolean) {
    isDialogOpen.value = false
    lastDialogCloseTime.value = Date.now()
    if (dialogResolve) {
      dialogResolve(result)
      dialogResolve = null
    }
  }

  function openSubview(subviewId: string, external = false) {
    activeSubview.value = subviewId
    subviewExternal.value = external
  }

  function closeSubview() {
    activeSubview.value = null
    subviewExternal.value = false
  }

  function setShortcutRecording(value: boolean) {
    shortcutRecording.value = value
  }

  function setFullscreenView(view: Component | null) {
    // markRaw：组件入 reactive store 防代理（Vue 对 reactive 组件有性能警告）
    fullscreenView.value = view ? markRaw(view) : null
  }

  function setShortcutError(id: string, error: string) {
    shortcutErrors.value = { ...shortcutErrors.value, [id]: error }
  }

  function clearShortcutError(id: string) {
    if (!(id in shortcutErrors.value)) return
    const next = { ...shortcutErrors.value }
    delete next[id]
    shortcutErrors.value = next
  }

  return {
    activeExtId,
    searchQuery,
    entryQuery,
    isComposing,
    isDialogOpen,
    dialogOptions,
    lastDialogCloseTime,
    suppressBlur,
    setActiveExtension,
    setSearchQuery,
    setComposing,
    showConfirm,
    resolveConfirm,
    activeSubview,
    subviewExternal,
    openSubview,
    closeSubview,
    shortcutRecording,
    setShortcutRecording,
    fullscreenView,
    setFullscreenView,
    fullscreenReturnExtId,
    extensionReturnExtId,
    selfTestMode,
    shortcutErrors,
    setShortcutError,
    clearShortcutError,
    showStatus,
  }
})

// ── app 行为（与 store 协作的副作用函数；扩展消费）──────────────────────
// 归位说明：原误放 utils/ 层（utils 应为无状态纯工具，不可反向依赖 stores）。

/** toast 反馈 + 立即隐藏主窗口（msg 为空则仅隐藏）。toast 为独立原生浮窗，在主窗
 *  原位继续展示至到期，无需等待。扩展「反馈后隐藏」通用动作。
 *  await 顺序化：两 invoke 并发发出的话，fetch IPC 通道不保证到达序——hide_window
 *  先到会把 Rust 侧状态置隐藏，乱序的 show_toast 拿到异常锚定输入。 */
export async function toastAndHide(msg?: string, opts?: { duration?: number }) {
  if (msg) {
    await useAppStore().showStatus(msg, { duration: opts?.duration })
  }
  hideWindow()
}

/** 复制文本到剪贴板 + toast 反馈 + 立即隐藏主窗口（复制型结果回车通用动作）。 */
export async function copyAndHide(value: string, label?: string) {
  await writeText(value)
  toastAndHide(label ?? t('common.copied'))
}

/**
 * 构造全局快捷键 onExecute：已可见且当前激活 → 隐藏窗口；否则激活扩展并清空搜索。
 * clipboard/translate/agent 的 globalShortcuts 共用。
 */
export function makeToggleHandler(extId: string, onActivate?: () => void) {
  return (wasVisible: boolean) => {
    const appStore = useAppStore()
    if (wasVisible && appStore.activeExtId === extId) {
      hideWindow()
      return
    }
    appStore.setActiveExtension(extId)
    appStore.setSearchQuery('')
    // 从隐藏呼出：setActiveExtension 已同步触发 Vue 视图更新（DOM 在下一 microtask 落地），
    // 再 invoke show——窗口渲染第一帧时已是目标扩展视图，避免先闪现主界面再切换
    if (!wasVisible) void showWindow()
    onActivate?.()
  }
}

/// SUPPRESS_BLUR_DELAY：原生面板（NSOpenPanel/NSColorPanel 等）关闭后窗口重获焦点前
/// 的过渡期，抑制失焦自动隐藏。800ms 覆盖系统面板动画 + 焦点回迁。
const SUPPRESS_BLUR_DELAY = 800

/// 主搜索框 DOM id（MainView 声明，扩展经 focusSearchInput 聚焦，解耦 DOM id 硬编码）
const SEARCH_INPUT_ID = 'main-search-input'

/** 聚焦主搜索框（若存在）。扩展切换 tab/动作后回焦用，封装 DOM id 查询。 */
export function focusSearchInput() {
  document.getElementById(SEARCH_INPUT_ID)?.focus()
}

/** 运行 fn 期间抑制失焦隐藏（原生面板打开场景）。无论成功/抛错，结束后延迟复位。 */
export async function withSuppressBlur<T>(fn: () => Promise<T> | T): Promise<T> {
  const appStore = useAppStore()
  appStore.suppressBlur = true
  try {
    return await fn()
  } finally {
    setTimeout(() => {
      appStore.suppressBlur = false
    }, SUPPRESS_BLUR_DELAY)
  }
}
