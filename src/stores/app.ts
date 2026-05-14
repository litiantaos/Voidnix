import { defineStore } from 'pinia'
import { ref } from 'vue'

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
  let dialogResolve: ((value: boolean) => void) | null = null

  const showSettings = ref(false)

  function setActiveModule(id: string | null) {
    activeModuleId.value = id
    showSettings.value = false // 切模块时重置设置面板
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

  function toggleSettings() {
    showSettings.value = !showSettings.value
  }

  return {
    activeModuleId,
    searchQuery,
    isComposing,
    isDialogOpen,
    dialogOptions,
    lastDialogCloseTime,
    setActiveModule,
    setSearchQuery,
    setComposing,
    setDialogOpen,
    showConfirm,
    resolveConfirm,
    showSettings,
    toggleSettings,
  }
})
