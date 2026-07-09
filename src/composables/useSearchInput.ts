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

/// 搜索输入处理：query 防抖、web 搜索/工具列表解析、默认结果加载、清空与回退。
/// 搜索状态（results/selectedIndex）由调用方持有并传入，便于与键盘导航共享。
export function useSearchInput(opts: SearchInputOptions) {
  const appStore = useAppStore()
  const { searchInput, results, selectedIndex, activeModule, restore, reset } = opts

  let searchTimeout: ReturnType<typeof setTimeout> | null = null
  let currentSearchId = 0
  // 模块模式 dynamic 的可取消句柄：新查询/离开模块时 abort 旧的，避免孤儿网络/IPC 请求
  let moduleAbort: AbortController | undefined
  // 进入模块前保存工具列表选中位置，退出回工具列表时恢复
  let savedToolIndex = 0

  const isLoading = ref(false)

  function clearSearch(value = '') {
    appStore.setSearchQuery(value)
    if (searchInput.value) searchInput.value.value = value
  }

  /** 激活模块：store.setActiveModule 自动快照入口 query。handleExecute 模块入口专用。 */
  function activateModule(moduleId: string) {
    appStore.setActiveModule(moduleId)
    clearSearch()
  }

  /** 退出模块 → 回到入口前状态：query 决定返回目标（/ → 工具列表，其余 → 主界面）。 */
  function exitModule() {
    const query = appStore.entryQuery
    moduleAbort?.abort()
    appStore.setActiveModule(null)
    if (query.startsWith('/')) {
      clearSearch(query)
      results.value = buildModuleResults()
      selectedIndex.value = savedToolIndex
      if (selectedIndex.value >= results.value.length) selectedIndex.value = 0
      restore('tools')
      searchInput.value?.focus()
      searchInput.value?.select()
    } else {
      clearSearch()
      loadDefaultResults()
    }
  }

  /** 强制回主界面（清空 query + 默认结果）。外部 subview ESC 专用。 */
  function goHome() {
    moduleAbort?.abort()
    appStore.setActiveModule(null)
    clearSearch()
    loadDefaultResults()
  }

  useTauriListener('app-cache-updated', () => {
    if (!appStore.activeModuleId && !appStore.searchQuery) {
      loadDefaultResults()
    }
  })

  // --- helpers ---

  /** 扩展 → 模块入口结果（回车走框架内置激活）。不产出 description：
   *  扩展列表为单行图标+名称，描述冗余，由 ContentView 的 v-if=item.description 自然过滤。 */
  function extToModuleResult(ext: Extension, score = 1000): SearchResult {
    return {
      id: `module-${ext.meta.id}`,
      title: ext.meta.name,
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
   *  每次 abort 上一次请求，避免连打时孤儿网络/IPC 查询堆积。
   *  异步 dynamic（ip/currency 网络）：进入即清空旧结果 + loading 占位，让用户看到「已进入模块、
   *  数据加载中」，而非残留工具列表等到网络返回才切换（先进去再加载）。
   *  同步 dynamic（time/uuid/base64）：即时填充，无 loading 闪烁。 */
  async function runModuleSearch(mod: Extension, query: string) {
    if (!mod.search) return
    moduleAbort?.abort()
    moduleAbort = new AbortController()
    const signal = moduleAbort.signal
    const searchId = ++currentSearchId

    const pending = mod.search.dynamic(query, { signal, moduleMode: true })

    // 同步 dynamic：即时填充，不闪 loading
    if (!(pending instanceof Promise)) {
      if (searchId === currentSearchId) {
        results.value = pending.map((r) => ({ ...r, module: mod.meta.id }))
        selectedIndex.value = 0
      }
      return
    }

    // 异步 dynamic：await 前同步清空 + loading（watch pre-flush，ContentView 渲染前生效）
    results.value = []
    selectedIndex.value = 0
    isLoading.value = true
    try {
      const res = await pending
      if (searchId === currentSearchId && !signal.aborted) {
        results.value = res.map((r) => ({ ...r, module: mod.meta.id }))
        selectedIndex.value = 0
      }
    } catch (e) {
      // abort 触发的 AbortError 是正常路径，静默；其余错误清空
      if (searchId === currentSearchId && (e as Error)?.name !== 'AbortError') {
        results.value = []
        selectedIndex.value = 0
      }
    } finally {
      if (searchId === currentSearchId) {
        isLoading.value = false
      }
    }
  }

  /** 用当前 searchQuery 重新调模块 dynamic 装填结果（ESC/tag 清空后回到模块默认列表）。 */
  function refreshModule() {
    const mod = activeModule.value
    if (!mod || mod.mainView || !mod.search) return
    runModuleSearch(mod, appStore.searchQuery)
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
      if (appStore.activeModuleId) {
        refreshModule()
      } else {
        loadDefaultResults()
      }
    } else if (appStore.activeModuleId) {
      exitModule()
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
    moduleAbort?.abort()
    window.removeEventListener('window-focused', focusHandler)
  })

  // 进入搜索型模块（无 mainView、有 search）：触发初始 dynamic 装填结果
  watch(
    () => activeModule.value?.meta.id,
    (newId, oldId) => {
      if (!newId || newId === oldId) return
      savedToolIndex = selectedIndex.value
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
    activateModule,
    goHome,
    handleTagClose,
    refreshModule,
    exitModule,
  }
}
