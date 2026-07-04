import { type Ref, type ComputedRef } from 'vue'
import { onKeyStroke } from '@/composables/events'
import { open } from '@tauri-apps/plugin-shell'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { getExtension } from '@/runtime/extension-registry'
import { useAppStore } from '@/stores/app'
import type { Extension, SearchResult } from '@/runtime/types'
import { hideWindow } from '@/utils/tauri'
import { buildSearchUrl, parseWebSearchQuery } from '@/utils/web-search'

interface ResultNavOptions {
  results: Ref<SearchResult[]>
  selectedIndex: Ref<number>
  activeModule: ComputedRef<Extension | null>
  clearSearch: (value?: string) => void
  loadDefaultResults: () => Promise<void>
  activateModule: (moduleId: string) => void
  goHome: () => void
  exitModule: () => void
}

/// 结果键盘导航：ArrowUp/Down 移动、Enter 执行分派、Escape 返回主界面/关闭窗口。
/// 搜索状态由 useSearchInput 持有，通过 opts 注入（clearSearch/loadDefaultResults 等）。
export function useResultNavigation(opts: ResultNavOptions) {
  const appStore = useAppStore()
  const {
    results,
    selectedIndex,
    clearSearch,
    loadDefaultResults,
    activateModule,
    goHome,
    exitModule,
  } = opts

  // --- execute ---

  async function handleExecute(result: SearchResult, _index?: number, e?: KeyboardEvent) {
    if (e) e.preventDefault()
    if (result.data?.kind === 'module' && result.data.moduleId) {
      activateModule(result.data.moduleId as string)
      return
    }
    // 扩展私有回车动作（result.module = 产出扩展 id，框架注入）
    const ext = getExtension(result.module)
    await ext?.onExecute?.(result)
    appStore.setActiveModule(null)
    hideWindow()
  }

  // --- keyboard ---

  function onKeydown(e: KeyboardEvent) {
    if (appStore.isComposing || e.isComposing || e.keyCode === 229) return

    switch (e.key) {
      case 'ArrowDown':
        if (appStore.activeModuleId) return
        e.preventDefault()
        if (results.value.length > 0) {
          selectedIndex.value =
            selectedIndex.value >= results.value.length - 1 ? 0 : selectedIndex.value + 1
        }
        break
      case 'ArrowUp':
        if (appStore.activeModuleId) return
        e.preventDefault()
        if (results.value.length > 0) {
          selectedIndex.value =
            selectedIndex.value <= 0 ? results.value.length - 1 : selectedIndex.value - 1
        }
        break
      case 'Enter':
        if (!appStore.activeModuleId && appStore.searchQuery.startsWith('//')) {
          const parsed = parseWebSearchQuery(appStore.searchQuery)
          if (parsed.type === 'url' || parsed.keyword) {
            e.preventDefault()
            open(buildSearchUrl(parsed)).catch(() => {})
            clearSearch()
            loadDefaultResults().finally(() => {
              hideWindow()
            })
          }
          return
        }
        if (appStore.activeModuleId) {
          // 模块模式下 Enter 由各 View / BaseList 自行处理（不介入）
          return
        }
        if (e.metaKey && results.value.length > 0) {
          const result = results.value[selectedIndex.value]
          if (result?.data?.path) {
            e.preventDefault()
            invoke(CMD.revealInFinder, { path: result.data.path })
            hideWindow()
            return
          }
        }
        e.preventDefault()
        e.stopImmediatePropagation()
        if (results.value.length > 0) {
          // H7：selectedIndex 可能因竞态（多选删除、模块动态返回较短结果）越界
          const result = results.value[selectedIndex.value]
          if (!result) return
          handleExecute(result)
        }
        break

      case 'Escape': {
        if (appStore.isDialogOpen) return

        // 表单控件的「先失焦」由设置型视图自行处理（useSettingsInput 在 capture 阶段
        // 拦截并 blur + stopImmediatePropagation）。此处统一为「退出当前层」——
        // 事件未被消费即意味着当前视图不是设置型，直接退出模块/子视图/窗口。
        if (appStore.activeSubview) {
          e.preventDefault()
          if (appStore.subviewExternal) {
            goHome()
          } else {
            appStore.closeSubview()
          }
          return
        }
        e.preventDefault()
        if (appStore.activeModuleId) {
          exitModule()
        } else {
          hideWindow()
        }
        break
      }
    }
  }

  onKeyStroke(['ArrowDown', 'ArrowUp', 'Enter', 'Escape'], onKeydown)

  return {
    onKeydown,
    handleExecute,
  }
}
