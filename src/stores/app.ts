import { defineStore } from 'pinia'
import { ref } from 'vue'
import { searchEngine } from '@/runtime/search-engine'
import { hideWindow } from '@/utils/tauri'
import { writeText } from '@/utils/clipboard'
import { showToast, type ToastOptions } from '@/composables/useToast'

export interface ConfirmOptions {
  title: string
  message?: string
  size?: 'sm' | 'md' | 'lg'
  kind?: 'warning' | 'info'
  okLabel?: string
  cancelLabel?: string
  showCancel?: boolean
}

export const useAppStore = defineStore('app', () => {
  const activeModuleId = ref<string | null>(null)
  const searchQuery = ref('')
  // 进入模块时的入口 query 快照：ESC 退出据此决定返回目标（/ → 工具列表，其余 → 主界面）
  const entryQuery = ref('')
  const isComposing = ref(false)

  const isDialogOpen = ref(false)
  const dialogOptions = ref<ConfirmOptions | null>(null)
  const lastDialogCloseTime = ref(0)
  // 原生系统对话框（如文件选择器）打开期间，抑制失焦隐藏（窗口重获焦点时自动解除）
  const suppressBlur = ref(false)
  let dialogResolve: ((value: boolean) => void) | null = null

  const activeSubview = ref<string | null>(null)
  // 子视图是否经由外部事件（open-module-subview）打开：外部打开时 ESC 直接回主界面，
  // 内部打开（从模块 mainView 进入 config 等）ESC 返回 mainView。
  const subviewExternal = ref(false)
  const shortcutRecording = ref(false)

  const shortcutErrors = ref<Record<string, string>>({})

  function showStatus(msg: string, opts?: ToastOptions) {
    showToast(msg, opts)
  }

  function setActiveModule(id: string | null) {
    const prevId = activeModuleId.value
    if (id && !prevId) {
      // 从外部进入模块：快照入口 query（统一所有激活路径——快捷键 toggle / open-module
      // 事件 / 扩展自激活等均经此，避免 useSearchInput 旁路漏存）
      entryQuery.value = searchQuery.value
    } else if (!id && prevId) {
      // 退出模块：清空
      entryQuery.value = ''
    }
    // module→module（如 OCR→translate）：保留原入口，ESC 回到最初进入点
    activeModuleId.value = id
    activeSubview.value = null
    subviewExternal.value = false
    // 模式切换：激活模块时 searchEngine 只调该模块 dynamic；null 恢复全局聚合。
    // 模块 onActivate/onDeactivate 由各 View 的 onActivated/onDeactivated（KeepAlive）承接。
    searchEngine.setActiveModule(id ?? undefined)
  }

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
    activeModuleId,
    searchQuery,
    entryQuery,
    isComposing,
    isDialogOpen,
    dialogOptions,
    lastDialogCloseTime,
    suppressBlur,
    setActiveModule,
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
    shortcutErrors,
    setShortcutError,
    clearShortcutError,
    showStatus,
  }
})

// ── app 行为（与 store 协作的副作用函数；扩展消费）──────────────────────
// 归位说明：原误放 utils/ 层（utils 应为无状态纯工具，不可反向依赖 stores）。

let hideTimer: ReturnType<typeof setTimeout> | null = null

/** 复制文本到剪贴板 + toast 反馈 + 延迟隐藏主窗口（复制型结果回车通用动作）。 */
export async function copyAndHide(value: string, label = '已复制') {
  if (hideTimer) {
    clearTimeout(hideTimer)
    hideTimer = null
  }
  await writeText(value)
  useAppStore().showStatus(label, { duration: 800 })
  hideTimer = setTimeout(() => {
    hideTimer = null
    hideWindow()
  }, 800)
}

/**
 * 构造全局快捷键 onExecute：已可见且当前激活 → 隐藏窗口；否则激活模块并清空搜索。
 * clipboard/translate/agent 的 globalShortcuts 共用。
 */
export function makeToggleHandler(moduleId: string, onActivate?: () => void) {
  return (wasVisible: boolean) => {
    const appStore = useAppStore()
    if (wasVisible && appStore.activeModuleId === moduleId) {
      hideWindow()
      return
    }
    appStore.setActiveModule(moduleId)
    appStore.setSearchQuery('')
    onActivate?.()
  }
}
