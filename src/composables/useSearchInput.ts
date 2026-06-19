import { ref, watch, type Ref, type ComputedRef, onMounted, onUnmounted } from 'vue'
import { useTauriListener } from '@/composables/useTauriListener'
import { searchEngine } from '@/runtime/search-engine'
import { getAllExtensions } from '@/runtime/extension-registry'
import { scoreFields } from '@/utils/fuzzy'
import { useAppStore } from '@/stores/app'
import type { Extension, SearchResult } from '@/runtime/types'
import { isTauri } from '@/utils/tauri'
import { buildOpenUrlResult, buildWebSearchResult, parseWebSearchQuery } from '@/utils/web-search'

interface SearchInputOptions {
  searchInput: Ref<HTMLInputElement | undefined>
  results: Ref<SearchResult[]>
  selectedIndex: Ref<number>
  activeModule: ComputedRef<Extension | null>
  restore: (key: string) => void
  reset: () => void
}

/// 搜索输入处理：query 防抖、web 搜索（//）/工具列表（/）解析、默认结果加载、清空与回退。
/// 搜索状态（results/selectedIndex）由调用方持有并传入，便于与键盘导航共享。
export function useSearchInput(opts: SearchInputOptions) {
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

  /** 搜索型模块（无 mainView、有 search）：调 module.search.dynamic，结果灌入共享 results。
   *  框架注入 module = 扩展 meta.id（与 searchEngine 一致）。 */
  async function runModuleSearch(mod: Extension, query: string) {
    if (!mod.search) return
    const searchId = ++currentSearchId
    isLoading.value = true
    try {
      const res = await mod.search.dynamic(query, { signal: new AbortController().signal })
      if (searchId === currentSearchId) {
        results.value = res.map((r) => ({ ...r, module: mod.meta.id }))
        selectedIndex.value = 0
      }
    } catch {
      if (searchId === currentSearchId) {
        results.value = []
        selectedIndex.value = 0
      }
    } finally {
      if (searchId === currentSearchId) {
        isLoading.value = false
      }
    }
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

    if (appStore.activeModuleId) {
      // 搜索型模块（无 mainView、有 search）：标准列表走 module.search.dynamic
      // mainView 模块自管列表（resolvedView），无 search 的模块无标准列表 → 均跳过
      const mod = activeModule.value
      if (!mod || mod.mainView || !mod.search) return
      if (searchTimeout) clearTimeout(searchTimeout)
      searchTimeout = setTimeout(() => runModuleSearch(mod, query), 100)
      return
    }

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

  // 进入搜索型模块（无 mainView、有 search）：触发初始 dynamic 装填结果
  watch(
    () => activeModule.value?.meta.id,
    (newId, oldId) => {
      if (!newId || newId === oldId) return
      const mod = activeModule.value
      if (!mod || mod.mainView || !mod.search) return
      runModuleSearch(mod, appStore.searchQuery)
    },
  )

  return {
    isLoading,
    onInput,
    clearSearch,
    loadDefaultResults,
    goBackToToolList,
    handleTagClose,
  }
}
