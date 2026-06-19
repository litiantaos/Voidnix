import { useAppStore } from '@/stores/app'
import { hideWindow } from '@/utils/tauri'

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
