import { ref, type Ref, type ComputedRef, onMounted, onUnmounted } from 'vue'
import { onKeyStroke } from '@/utils/events'
import { open } from '@tauri-apps/plugin-shell'
import { invoke } from '@tauri-apps/api/core'
import { useTauriListener } from '@/composables/useTauriListener'
import { searchAll, executeResult } from '@/core/module-registry'
import { getVisibleModules, moduleToSearchResult } from '@/core/module-helpers'
import { scoreFields } from '@/utils/fuzzy'
import { useAppStore } from '@/stores/app'
import type { SearchResult } from '@/types/module'
import { isTauri, hideWindow } from '@/utils/tauri'
import type { AppModule } from '@/types/module'

interface Options {
  searchInput: Ref<HTMLInputElement | undefined>
  results: Ref<SearchResult[]>
  selectedIndex: Ref<number>
  activeModule: ComputedRef<AppModule | null>
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

  interface WebSearchQuery {
    type: 'search' | 'url'
    engine?: 'google' | 'bing'
    keyword: string
    url?: string
  }

  function parseWebSearchQuery(rawQuery: string): WebSearchQuery {
    const isBing = rawQuery.startsWith('//b ') || rawQuery === '//b'
    const keyword = rawQuery.startsWith('//b ')
      ? rawQuery.slice(4).trim()
      : rawQuery === '//b'
        ? ''
        : rawQuery.slice(2).trim()

    if (!keyword) return { type: 'search', engine: isBing ? 'bing' : 'google', keyword: '' }

    if (/^https?:\/\//.test(keyword)) {
      return { type: 'url', keyword: '', url: keyword }
    }

    if (
      /^[\w.-]+\.[a-z]{2,}(\/.*)?$/i.test(keyword) ||
      /^\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}(:\d+)?(\/.*)?$/.test(keyword)
    ) {
      return { type: 'url', keyword: '', url: `https://${keyword}` }
    }

    return { type: 'search', engine: isBing ? 'bing' : 'google', keyword }
  }

  function buildModuleResults(): SearchResult[] {
    return getVisibleModules().map((m) => moduleToSearchResult(m, 1000))
  }

  async function loadDefaultResults() {
    if (!isTauri) return
    const searchId = ++currentSearchId
    isLoading.value = true
    try {
      const defaultResults = await searchAll('')
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
    await executeResult(result)
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
        results.value = [
          {
            id: 'open-url',
            title: '打开链接',
            description: parsed.url!,
            icon: 'i-ri-links-line',
            module: 'system',
            score: -1,
            data: { kind: 'open-url', url: parsed.url! },
          },
        ]
        selectedIndex.value = 0
        return
      }

      const engine = parsed.engine === 'bing' ? 'Bing' : 'Google'
      const desc =
        parsed.engine === 'bing' ? '在默认浏览器中打开' : '在默认浏览器中打开，//b 可使用 Bing 搜索'

      results.value = [
        {
          id: 'web-search',
          title: `${engine} 搜索`,
          description: desc,
          icon: 'i-ri-earth-line',
          module: 'system',
          score: -1,
          data: { kind: 'web-search', engine: parsed.engine, keyword: parsed.keyword },
        },
      ]
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

      const modules = getVisibleModules()
      const matchedModules = modules
        .map((m) => ({
          module: m,
          score: scoreFields([m.name, m.id, m.description, ...m.keywords], keyword),
        }))
        .filter((item) => item.score > 0)
        .sort((a, b) => b.score - a.score)

      results.value = matchedModules.map(({ module: m, score }) => moduleToSearchResult(m, score))

      if (selectedIndex.value >= results.value.length) selectedIndex.value = 0
      return
    }

    const searchId = ++currentSearchId

    if (appStore.activeModuleId) return

    if (query.trim()) {
      searchTimeout = setTimeout(async () => {
        try {
          let batchTimer: ReturnType<typeof setTimeout> | null = null
          let batched: SearchResult[] = []

          const applyResults = (newResults: SearchResult[]) => {
            if (searchId !== currentSearchId) return
            results.value = newResults
            selectedIndex.value = 0
          }

          const flush = () => {
            batchTimer = null
            applyResults(batched)
          }

          const onUpdate = (r: SearchResult[]) => {
            if (searchId !== currentSearchId) return
            batched = r
            if (batchTimer) clearTimeout(batchTimer)
            batchTimer = setTimeout(flush, 30)
          }

          const finalResults = await searchAll(query, onUpdate)
          if (batchTimer) {
            clearTimeout(batchTimer)
            batchTimer = null
          }
          applyResults(finalResults)
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
          if (parsed.type === 'url') {
            e.preventDefault()
            open(parsed.url!).catch(() => {})
            clearSearch()
            loadDefaultResults().finally(() => {
              hideWindow()
            })
            return
          }
          const keyword = parsed.keyword
          if (keyword) {
            e.preventDefault()
            const url =
              parsed.engine === 'bing'
                ? `https://www.bing.com/search?q=${encodeURIComponent(keyword)}`
                : `https://www.google.com/search?q=${encodeURIComponent(keyword)}`
            open(url).catch(() => {})
            clearSearch()
            loadDefaultResults().finally(() => {
              hideWindow()
            })
          }
          return
        }
        if (appStore.activeModuleId) {
          if (activeModule.value?.disableSearchInput) return
          if (activeModule.value?.useSearchInput && activeModule.value?.onSearchInput) {
            const query = appStore.searchQuery.trim()
            if (query) {
              e.preventDefault()
              activeModule.value.onSearchInput(query)
              if (!activeModule.value.keepSearchInput) {
                clearSearch()
              }
            }
          }
          return
        }
        if (e.metaKey && results.value.length > 0) {
          const result = results.value[selectedIndex.value]
          if (result?.data?.path) {
            e.preventDefault()
            invoke('reveal_in_finder', { path: result.data.path })
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
