import { ref, type Ref, type ComputedRef, onMounted, onUnmounted } from 'vue'
import { onKeyStroke } from '@vueuse/core'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import {
  searchAll,
  executeResult,
  getAllModules,
} from '@/core/module-registry'
import { useAppStore } from '@/stores/app'
import { useModulesStore } from '@/stores/modules'
import type { SearchResult } from '@/types/module'
import { isTauri } from '@/utils/tauri'
import type { AppModule } from '@/types/module'

interface Options {
  searchInput: Ref<HTMLInputElement | undefined>
  results: Ref<SearchResult[]>
  selectedIndex: Ref<number>
  activeModule: ComputedRef<AppModule | null>
  save: (key: string) => void
  restore: (key: string) => void
  reset: () => void
}

export function useSearchCommand(opts: Options) {
  const appStore = useAppStore()
  const modulesStore = useModulesStore()
  const { searchInput, results, selectedIndex, activeModule, save, restore, reset } = opts

  let searchTimeout: ReturnType<typeof setTimeout> | null = null
  let currentSearchId = 0
  let unlistenCacheUpdated: (() => void) | undefined

  const isLoading = ref(false)

  // --- helpers ---

  function buildModuleResults(): SearchResult[] {
    return getAllModules()
      .filter((m) => !m.hidden)
      .sort((a, b) => (a.order ?? 9999) - (b.order ?? 9999))
      .map((m) => ({
        id: `module-${m.id}`,
        title: m.name,
        description: m.description,
        icon: m.icon,
        module: 'system',
        score: 1000,
        data: { kind: 'module', moduleId: m.id },
      }))
  }

  async function loadDefaultResults() {
    if (!isTauri) return
    isLoading.value = true
    try {
      results.value = await searchAll('')
      selectedIndex.value = 0
    } catch {
      results.value = []
    } finally {
      isLoading.value = false
    }
  }

  // --- execute ---

  async function handleExecute(result: SearchResult, _index?: number, e?: KeyboardEvent) {
    if (e) e.preventDefault()
    if (result.data?.kind === 'module' && result.data.moduleId) {
      save('tools')
      reset()
      appStore.setActiveModule(result.data.moduleId as string)
      appStore.setSearchQuery('')
      if (searchInput.value) searchInput.value.value = ''
      return
    }
    await executeResult(result)
    appStore.setActiveModule(null)
    invoke('hide_window').catch(() => {})
  }

  // --- input ---

  async function onInput(e: Event) {
    const query = (e.target as HTMLInputElement).value
    const wasToolListMode = appStore.searchQuery.startsWith('/')
    appStore.setSearchQuery(query)

    if (searchTimeout) clearTimeout(searchTimeout)

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

      const modules = getAllModules()
        .filter((m) => !m.hidden)
        .sort((a, b) => (a.order ?? 9999) - (b.order ?? 9999))

      try {
        const itemsToScore = modules.map(
          (m) => `${m.name} ${m.id} ${m.description} ${m.keywords.join(' ')}`,
        )
        const scores = await invoke<number[]>('score_items', {
          query: keyword,
          items: itemsToScore,
        })

        const matchedModules = modules
          .map((m, i) => ({ module: m, score: scores[i] }))
          .filter((item) => item.score > 0)
          .sort((a, b) => b.score - a.score)

        results.value = matchedModules.map(({ module: m, score }) => ({
          id: `module-${m.id}`,
          title: m.name,
          description: m.description,
          icon: m.icon,
          module: 'system',
          score,
          data: { kind: 'module', moduleId: m.id },
        }))

        if (selectedIndex.value >= results.value.length) selectedIndex.value = 0
      } catch (e) {
        console.error('Failed to score items:', e)
      }
      return
    }

    const searchId = ++currentSearchId

    if (appStore.activeModuleId) return

    if (query.trim()) {
      searchTimeout = setTimeout(async () => {
        try {
          const finalResults = await searchAll(query, (r) => {
            if (searchId === currentSearchId) {
              results.value = r
              if (selectedIndex.value >= r.length) selectedIndex.value = 0
            }
          })
          if (searchId === currentSearchId) {
            results.value = finalResults
            if (selectedIndex.value >= finalResults.length) selectedIndex.value = 0
          }
        } catch {
          if (searchId === currentSearchId) {
            results.value = []
            selectedIndex.value = 0
          }
        }
      }, 100)
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
            selectedIndex.value >= results.value.length - 1
              ? 0
              : selectedIndex.value + 1
        }
        break
      case 'ArrowUp':
        if (appStore.activeModuleId) return
        e.preventDefault()
        if (results.value.length > 0) {
          selectedIndex.value =
            selectedIndex.value <= 0
              ? results.value.length - 1
              : selectedIndex.value - 1
        }
        break
      case 'Enter':
        if (appStore.activeModuleId) {
          if (activeModule.value?.multiline) return
          if (
            activeModule.value?.useSearchInput &&
            activeModule.value?.onSearchInput
          ) {
            const query = appStore.searchQuery.trim()
            if (query) {
              e.preventDefault()
              activeModule.value.onSearchInput(query)
              if (!activeModule.value.keepSearchInput) {
                appStore.setSearchQuery('')
                if (searchInput.value) searchInput.value.value = ''
              }
            }
          }
          return
        }
        e.preventDefault()
        e.stopImmediatePropagation()
        if (results.value.length > 0) {
          handleExecute(results.value[selectedIndex.value])
        }
        break
      case 'Backspace': {
        const el = document.activeElement
        if (
          el?.tagName === 'INPUT' ||
          el?.tagName === 'TEXTAREA' ||
          el?.tagName === 'SELECT' ||
          el?.hasAttribute('contenteditable')
        ) {
          return
        }

        if (appStore.activeModuleId && !appStore.searchQuery) {
          e.preventDefault()
          appStore.setActiveModule(null)
          appStore.setSearchQuery('/')
          if (searchInput.value) searchInput.value.value = '/'

          results.value = buildModuleResults()
          if (selectedIndex.value >= results.value.length) selectedIndex.value = 0
          restore('tools')
        }
        break
      }
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

        if (appStore.showPanel) {
          e.preventDefault()
          appStore.showPanel = false
          return
        }
        e.preventDefault()
        if (appStore.activeModuleId) {
          if (appStore.searchQuery) {
            appStore.setSearchQuery('')
            if (searchInput.value) searchInput.value.value = ''
          } else {
            appStore.setActiveModule(null)
            appStore.setSearchQuery('/')
            if (searchInput.value) searchInput.value.value = '/'

            results.value = buildModuleResults()
            if (selectedIndex.value >= results.value.length) selectedIndex.value = 0
            restore('tools')
          }
        } else if (appStore.searchQuery) {
          appStore.setSearchQuery('')
          if (searchInput.value) searchInput.value.value = ''
          loadDefaultResults()
          searchInput.value?.focus()
        } else {
          invoke('hide_window').catch(() => {})
        }
        break
      }
    }
  }

  onKeyStroke(['ArrowDown', 'ArrowUp', 'Enter', 'Backspace', 'Escape'], onKeydown)

  // --- tag & focus ---

  const handleTagClose = () => {
    if (appStore.searchQuery) {
      appStore.setSearchQuery('')
      if (searchInput.value) searchInput.value.value = ''
    } else {
      appStore.setActiveModule(null)
      appStore.setSearchQuery('/')
      if (searchInput.value) searchInput.value.value = '/'

      results.value = buildModuleResults()
      if (selectedIndex.value >= results.value.length) selectedIndex.value = 0
      restore('tools')
    }
    searchInput.value?.focus()
  }

  const focusHandler = () => {
    if (activeModule.value?.multiline) return
    searchInput.value?.focus()
    if (appStore.searchQuery) searchInput.value?.select()
  }

  onMounted(async () => {
    modulesStore.loadModules()
    if (!activeModule.value?.multiline) searchInput.value?.focus()
    await loadDefaultResults()
    window.addEventListener('window-focused', focusHandler)

    unlistenCacheUpdated = await listen('app-cache-updated', () => {
      if (!appStore.activeModuleId && !appStore.searchQuery) {
        loadDefaultResults()
      }
    })
  })

  onUnmounted(() => {
    window.removeEventListener('window-focused', focusHandler)
    unlistenCacheUpdated?.()
  })

  return {
    onInput,
    handleExecute,
    handleTagClose,
    loadDefaultResults,
    isLoading,
  }
}
