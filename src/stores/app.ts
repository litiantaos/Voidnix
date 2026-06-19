import { defineStore } from 'pinia'
import { ref } from 'vue'
import { searchEngine } from '@/runtime/search-engine'

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
