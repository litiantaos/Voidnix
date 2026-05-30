import { defineStore } from 'pinia'
import { ref } from 'vue'
import { getModule } from '@/core/module-registry'

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

  const showPanel = ref(false)
  const shortcutRecording = ref(false)

  const shortcutErrors = ref<Record<string, string>>({})

  function setActiveModule(id: string | null) {
    const oldMod = activeModuleId.value ? getModule(activeModuleId.value) : null
    const newMod = id ? getModule(id) : null

    activeModuleId.value = id
    showPanel.value = false

    if (oldMod?.onDeactivate) oldMod.onDeactivate()
    if (newMod?.onActivate) newMod.onActivate()
  }

  function setSearchQuery(query: string) {
    searchQuery.value = query
  }

  function setComposing(status: boolean) {
    isComposing.value = status
  }

  function setDialogOpen(status: boolean) {
    isDialogOpen.value = status
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

  function togglePanel() {
    showPanel.value = !showPanel.value
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
    setDialogOpen,
    showConfirm,
    resolveConfirm,
    showPanel,
    togglePanel,
    shortcutRecording,
    setShortcutRecording,
    shortcutErrors,
    setShortcutError,
    clearShortcutError,
  }
})
