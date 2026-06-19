import { defineStore } from 'pinia'
import { ref } from 'vue'
import { searchEngine } from '@/runtime/search-engine'
import { hideWindow } from '@/utils/tauri'
import { writeText } from '@/utils/clipboard'

export interface ConfirmOptions {
  title: string
  message?: string
  size?: 'sm' | 'md' | 'lg'
  kind?: 'warning' | 'info'
  okLabel?: string
  cancelLabel?: string
  showCancel?: boolean
  showFooter?: boolean
}

export const useAppStore = defineStore('app', () => {
  const activeModuleId = ref<string | null>(null)
  const searchQuery = ref('')
  const isComposing = ref(false)

  const isDialogOpen = ref(false)
  const dialogOptions = ref<ConfirmOptions | null>(null)
  const lastDialogCloseTime = ref(0)
  // 原生系统对话框（如文件选择器）打开期间，抑制失焦隐藏
  const suppressBlur = ref(false)
  let dialogResolve: ((value: boolean) => void) | null = null

  const activeSubview = ref<string | null>(null)
  const shortcutRecording = ref(false)

  const shortcutErrors = ref<Record<string, string>>({})

  // 状态栏瞬时消息
  const statusMessage = ref('')
  let statusTimer: ReturnType<typeof setTimeout> | null = null

  function showStatus(msg: string, duration = 2000) {
    statusMessage.value = msg
    if (statusTimer) clearTimeout(statusTimer)
    if (duration > 0) {
      statusTimer = setTimeout(() => {
        statusMessage.value = ''
        statusTimer = null
      }, duration)
    }
  }

  function clearStatus() {
    statusMessage.value = ''
    if (statusTimer) {
      clearTimeout(statusTimer)
      statusTimer = null
    }
  }

  function setActiveModule(id: string | null) {
    activeModuleId.value = id
    activeSubview.value = null
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

  function openSubview(subviewId: string) {
    activeSubview.value = subviewId
  }

  function closeSubview() {
    activeSubview.value = null
  }

  function toggleSubview(subviewId: string) {
    activeSubview.value = activeSubview.value === subviewId ? null : subviewId
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
    openSubview,
    closeSubview,
    toggleSubview,
    shortcutRecording,
    setShortcutRecording,
    shortcutErrors,
    setShortcutError,
    clearShortcutError,
    statusMessage,
    showStatus,
    clearStatus,
  }
})

// ── app 行为（与 store 协作的副作用函数；扩展消费）──────────────────────
// 归位说明：原误放 utils/ 层（utils 应为无状态纯工具，不可反向依赖 stores）。

let hideTimer: ReturnType<typeof setTimeout> | null = null

/** 复制文本到剪贴板 + 状态栏反馈 + 延迟隐藏主窗口（复制型结果回车通用动作）。 */
export async function copyAndHide(value: string, label = '已复制') {
  if (hideTimer) {
    clearTimeout(hideTimer)
    hideTimer = null
  }
  await writeText(value)
  useAppStore().showStatus(label, 800)
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
