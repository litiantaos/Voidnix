import { ref, type Ref, type ComputedRef, onMounted, onUnmounted } from 'vue'
import { onKeyStroke } from '@/composables/events'
import { open } from '@tauri-apps/plugin-shell'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { useTauriListener } from '@/composables/useTauriListener'
import { searchEngine } from '@/runtime/search-engine'
import { getAllExtensions, getExtension } from '@/runtime/extension-registry'
import { scoreFields } from '@/utils/fuzzy'
import { useAppStore } from '@/stores/app'
import type { SearchResult, Extension } from '@/runtime/types'
import { isTauri, hideWindow } from '@/utils/tauri'
import {
  parseWebSearchQuery,
  buildWebSearchResult,
  buildOpenUrlResult,
  buildSearchUrl,
} from '@/utils/web-search'

interface Options {
  searchInput: Ref<HTMLInputElement | undefined>
  results: Ref<SearchResult[]>
  selectedIndex: Ref<number>
  activeModule: ComputedRef<Extension | null>
  restore: (key: string) => void
  reset: () => void
}

export function useSearchCommand(opts: Options) {
  const appStore = useAppStore()
  const { searchInput, results, selectedIndex, activeModule, restore, reset } = opts

  let searchTimeout: ReturnType<typeof setTimeout> | null = null
  let currentSearchId = 0

  const isLoading = ref(false)

  function clearSearch(value = '') {
    appStore.setSearchQuery(value)
    if (searchInput.value) searchInput.value.value = value
  }

  function goBackToToolList() {
    appStore.setActiveModule(null)
    clearSearch('/')
    results.value = buildModuleResults()
    if (selectedIndex.value >= results.value.length) selectedIndex.value = 0
    restore('tools')
  }

  useTauriListener('app-cache-updated', () => {
    if (!appStore.activeModuleId && !appStore.searchQuery) {
      loadDefaultResults()
    }
  })

  // --- helpers ---

  /** 扩展 → 模块入口结果（回车走框架内置激活，§2.2 执行分派） */
  function extToModuleResult(ext: Extension, score = 1000): SearchResult {
    return {
      id: `module-${ext.meta.id}`,
      title: ext.meta.name,
      description: ext.meta.description,
      icon: ext.meta.icon,
      module: ext.meta.id,
      score,
      data: { kind: 'module', moduleId: ext.meta.id },
    }
  }

  /** 可见扩展（非 hidden），按 order 排序 */
  function getVisibleExtensions(): Extension[] {
    return getAllExtensions()
      .filter((e) => !e.meta.hidden)
      .sort((a, b) => a.meta.order - b.meta.order)
  }

  function buildModuleResults(): SearchResult[] {
    return getVisibleExtensions().map((e) => extToModuleResult(e, 1000))
  }

  async function loadDefaultResults() {
    if (!isTauri) return
    const searchId = ++currentSearchId
    isLoading.value = true
    try {
      const defaultResults = await searchEngine.search('')
      if (searchId === currentSearchId) {
        results.value = defaultResults
        selectedIndex.value = 0
      }
    } catch {
      if (searchId === currentSearchId) {
        results.value = []
      }
    } finally {
      if (searchId === currentSearchId) {
        isLoading.value = false
      }
    }
  }

  // --- execute ---

  async function handleExecute(result: SearchResult, _index?: number, e?: KeyboardEvent) {
    if (e) e.preventDefault()
    if (result.data?.kind === 'module' && result.data.moduleId) {
      appStore.setActiveModule(result.data.moduleId as string)
      clearSearch()
      return
    }
    // 扩展私有回车动作（result.module = 产出扩展 id，框架注入）
    const ext = getExtension(result.module)
    await ext?.onExecute?.(result)
    appStore.setActiveModule(null)
    hideWindow()
  }

  // --- input ---

  async function onInput(e: Event) {
    const query = (e.target as HTMLInputElement).value
    const wasToolListMode = appStore.searchQuery.startsWith('/')
    appStore.setSearchQuery(query)
    if (searchTimeout) clearTimeout(searchTimeout)

    if (!appStore.activeModuleId && query.startsWith('//')) {
      const parsed = parseWebSearchQuery(query)

      if (parsed.type === 'url') {
        results.value = [buildOpenUrlResult(parsed.url!)]
        selectedIndex.value = 0
        return
      }

      results.value = [buildWebSearchResult(parsed)]
      selectedIndex.value = 0
      return
    }

    if (!appStore.activeModuleId && query.startsWith('/')) {
      if (!wasToolListMode) {
        reset()
        selectedIndex.value = 0
      }
      const keyword = query.slice(1).trim().toLowerCase()

      if (!keyword) {
        results.value = buildModuleResults()
        if (selectedIndex.value >= results.value.length) selectedIndex.value = 0
        return
      }

      const matchedExts = getVisibleExtensions()
        .map((ext) => ({
          ext,
          score: scoreFields(
            [ext.meta.name, ext.meta.id, ext.meta.description, ...(ext.meta.keywords ?? [])],
            keyword,
          ),
        }))
        .filter((item) => item.score > 0)
        .sort((a, b) => b.score - a.score)

      results.value = matchedExts.map(({ ext, score }) => extToModuleResult(ext, score))

      if (selectedIndex.value >= results.value.length) selectedIndex.value = 0
      return
    }

    const searchId = ++currentSearchId

    if (appStore.activeModuleId) return

    if (query.trim()) {
      searchTimeout = setTimeout(async () => {
        try {
          const finalResults = await searchEngine.search(query)
          if (searchId === currentSearchId) {
            results.value = finalResults
            selectedIndex.value = 0
          }
        } catch {
          if (searchId === currentSearchId) {
            results.value = []
            selectedIndex.value = 0
          }
        }
      }, 50)
    } else {
      await loadDefaultResults()
    }
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
          // 模块模式下 Enter 由各 View / BaseList 自行处理（useSearchCommand 不介入）
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
          handleExecute(results.value[selectedIndex.value])
        }
        break

      case 'Escape': {
        if (appStore.isDialogOpen) return

        const el = document.activeElement
        const isFormControl =
          el?.tagName === 'SELECT' ||
          el?.tagName === 'INPUT' ||
          el?.tagName === 'TEXTAREA' ||
          el?.hasAttribute('contenteditable') ||
          el?.hasAttribute('data-settings-control')

        if (isFormControl) {
          if (el === searchInput.value) {
            // allow standard behavior to proceed
          } else {
            if (el instanceof HTMLElement) el.blur()
            e.preventDefault()
            return
          }
        }

        if (appStore.activeSubview) {
          e.preventDefault()
          appStore.closeSubview()
          return
        }
        e.preventDefault()
        if (appStore.activeModuleId) {
          if (appStore.searchQuery) {
            clearSearch()
          } else {
            goBackToToolList()
          }
        } else if (appStore.searchQuery) {
          clearSearch()
          loadDefaultResults()
          searchInput.value?.focus()
        } else {
          hideWindow()
        }
        break
      }
    }
  }

  onKeyStroke(['ArrowDown', 'ArrowUp', 'Enter', 'Escape'], onKeydown)

  // --- tag & focus ---

  const handleTagClose = () => {
    if (appStore.searchQuery) {
      clearSearch()
      loadDefaultResults()
    } else if (appStore.activeModuleId) {
      goBackToToolList()
    }
    searchInput.value?.focus()
  }

  const focusHandler = async () => {
    if (activeModule.value?.disableSearchInput) return
    searchInput.value?.focus()
    if (appStore.searchQuery) searchInput.value?.select()
    if (!appStore.activeModuleId && !appStore.searchQuery) {
      await loadDefaultResults()
    }
  }

  onMounted(async () => {
    if (!activeModule.value?.disableSearchInput) searchInput.value?.focus()
    await loadDefaultResults()
    window.addEventListener('window-focused', focusHandler)
  })

  onUnmounted(() => {
    window.removeEventListener('window-focused', focusHandler)
  })

  return {
    onInput,
    handleExecute,
    handleTagClose,
    loadDefaultResults,
    isLoading,
  }
}
