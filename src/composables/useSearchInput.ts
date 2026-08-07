import { ref, watch, type Ref, type ComputedRef, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useTauriListener } from '@/composables/useTauriListener'
import { searchEngine } from '@/runtime/search-engine'
import { getAllExtensions } from '@/runtime/extension-registry'
import { scoreExtensionEntry } from '@/utils/fuzzy'
import { useAppStore } from '@/stores/app'
import { CMD } from '@/commands'
import type { Extension, SearchResult } from '@/runtime/types'
import { isTauri } from '@/utils/tauri'
import { buildOpenUrlResult, buildWebSearchResult, parseWebSearchQuery } from '@/utils/web-search'
import { probeMem, trackResults } from '@/utils/mem-probe'

interface SearchInputOptions {
  searchInput: Ref<HTMLInputElement | undefined>
  results: Ref<SearchResult[]>
  selectedIndex: Ref<number>
  activeExtension: ComputedRef<Extension | null>
  reset: () => void
}

/// 搜索输入处理：query 防抖、web 搜索/工具列表解析、默认结果加载、清空与回退。
/// 全局与搜索型扩展均走 searchEngine 单通道（超时/abort/extId 注入统一）；本 composable 只做 UX 外壳。
/// 搜索状态（results/selectedIndex）由调用方持有并传入，便于与键盘导航共享。
export function useSearchInput(opts: SearchInputOptions) {
  const appStore = useAppStore()
  const { searchInput, results, selectedIndex, activeExtension, reset } = opts

  let searchTimeout: ReturnType<typeof setTimeout> | null = null
  let currentSearchId = 0
  // 进入扩展前保存工具列表选中位置，退出回工具列表时恢复
  let savedToolIndex = 0

  const isLoading = ref(false)

  /** 增量结果到达时保留用户已做的导航：selectedIndex 仍在新列表有效范围内则不动，
   *  仅越界时回 0。避免慢扩展（mdfind）的增量 flush 打断用户方向键导航。 */
  function clampSelected(len: number) {
    if (selectedIndex.value >= len) selectedIndex.value = 0
  }

  function clearSearch(value = '') {
    appStore.setSearchQuery(value)
    if (searchInput.value) searchInput.value.value = value
  }

  /** 激活扩展：store.setActiveExtension 自动快照入口 query。handleExecute 扩展入口专用。 */
  function activateExtension(extId: string) {
    appStore.setActiveExtension(extId)
    clearSearch()
  }

  /** 退出扩展 → 回到入口前状态：query 决定返回目标（/ → 工具列表，其余 → 主界面）。
   *  滚动位置由 setActiveExtension(null) 触发 scrollKey watch 自动 save/restore，此处不再手动处理。
   *  / 分支先 clearSearch(query) 再 setActiveExtension(null)：让 scrollKey 单调 ext→tools 变化，
   *  避免 setActiveExtension 先行产生 ext→home 中转态（虽 Vue 批处理合并 watch 不致误触发 clear，
   *  但线性转换更稳健、不依赖 flush 时序细节）。setActiveExtension 退出分支不读 searchQuery，顺序安全。 */
  function exitExtension() {
    const query = appStore.entryQuery
    searchEngine.abort()
    if (query.startsWith('/')) {
      clearSearch(query)
      appStore.setActiveExtension(null)
      results.value = buildToolListResults(query)
      selectedIndex.value = savedToolIndex
      if (selectedIndex.value >= results.value.length) selectedIndex.value = 0
      searchInput.value?.focus()
      searchInput.value?.select()
    } else {
      appStore.setActiveExtension(null)
      clearSearch()
      loadDefaultResults(true)
    }
  }

  /** 强制回主界面（清空 query + 默认结果）。外部 subview ESC 专用。 */
  function goHome() {
    searchEngine.abort()
    appStore.setActiveExtension(null)
    clearSearch()
    loadDefaultResults(true)
  }

  useTauriListener('app-cache-updated', () => {
    if (!appStore.activeExtId && !appStore.searchQuery) {
      loadDefaultResults()
    }
  })

  // 图标就绪（后台提取完成）：补全图标后刷新默认列表
  // 后台批量提取时可能频繁触发，trailing debounce 合并为一次重载
  let iconTimer: ReturnType<typeof setTimeout> | undefined
  useTauriListener('app-icons-updated', () => {
    if (appStore.activeExtId || appStore.searchQuery) return
    clearTimeout(iconTimer)
    iconTimer = setTimeout(() => loadDefaultResults(), 150)
  })

  // --- helpers ---

  /** 扩展 → 扩展入口结果（回车走框架内置激活）。不产出 description：
   *  扩展列表为单行图标+名称，描述冗余，由 ContentView 的 v-if=item.description 自然过滤。 */
  function extToEntryResult(ext: Extension, score = 1000): SearchResult {
    return {
      id: `ext-entry-${ext.meta.id}`,
      title: ext.meta.name,
      icon: ext.meta.icon,
      extId: ext.meta.id,
      score,
      data: { kind: 'extension', extId: ext.meta.id },
    }
  }

  /** 可见扩展（非 hidden），按 order 排序 */
  function getVisibleExtensions(): Extension[] {
    return getAllExtensions()
      .filter((e) => !e.meta.hidden)
      .sort((a, b) => a.meta.order - b.meta.order)
  }

  function buildExtensionList(): SearchResult[] {
    return getVisibleExtensions().map((e) => extToEntryResult(e, 1000))
  }

  /** `/` 工具列表结果：空关键词全量 order 序；有关键词则 scoreExtensionEntry 过滤排序。
   *  onInput 与 exitExtension 共用，避免退出扩展后丢过滤。 */
  function buildToolListResults(query: string): SearchResult[] {
    const keyword = query.slice(1).trim().toLowerCase()
    if (!keyword) return buildExtensionList()

    return getVisibleExtensions()
      .map((ext) => ({ ext, score: scoreExtensionEntry(ext.meta, keyword) }))
      .filter((item) => item.score > 0)
      .sort((a, b) => b.score - a.score)
      .map(({ ext, score }) => extToEntryResult(ext, score))
  }

  async function loadDefaultResults(resetSelection = false) {
    if (!isTauri) return
    const searchId = ++currentSearchId
    // 转移入口（退出扩展/回主页/清空输入）显式归首项；后台刷新（图标就绪/缓存变更/窗口获焦）
    // 保留用户已有导航，由 clampSelected 在结果到达时兜底越界。
    if (resetSelection) selectedIndex.value = 0
    try {
      const defaultResults = await searchEngine.search('', (partial) => {
        if (searchId === currentSearchId) {
          results.value = partial
          clampSelected(partial.length)
        }
      })
      if (searchId === currentSearchId) {
        results.value = defaultResults
        clampSelected(defaultResults.length)
      }
    } catch {
      if (searchId === currentSearchId) {
        results.value = []
        selectedIndex.value = 0
      }
    }
  }

  /** 搜索型扩展：统一走 searchEngine.search（activeExtension 已由 setActiveExtension 注入）。
   *  延迟 loading：同步 dynamic 通常 <50ms 不闪；网络/IPC 超过阈值才显示占位。 */
  async function runExtensionSearch(query: string) {
    const searchId = ++currentSearchId
    const loadingTimer = setTimeout(() => {
      if (searchId === currentSearchId) isLoading.value = true
    }, 50)
    try {
      const res = await searchEngine.search(query, (partial) => {
        if (searchId === currentSearchId) {
          results.value = partial
          clampSelected(partial.length)
        }
      })
      if (searchId === currentSearchId) {
        results.value = res
        clampSelected(res.length)
      }
    } catch {
      if (searchId === currentSearchId) {
        results.value = []
        selectedIndex.value = 0
      }
    } finally {
      clearTimeout(loadingTimer)
      if (searchId === currentSearchId) isLoading.value = false
    }
  }

  /** 用当前 searchQuery 重新调扩展 dynamic 装填结果（ESC/tag 清空后回到扩展默认列表）。 */
  function refreshExtension() {
    const ext = activeExtension.value
    if (!ext || ext.mainView || !ext.search) return
    runExtensionSearch(appStore.searchQuery)
  }

  // --- input ---

  async function onInput(e: Event) {
    const query = (e.target as HTMLInputElement).value
    const wasToolListMode = appStore.searchQuery.startsWith('/')
    appStore.setSearchQuery(query)
    if (searchTimeout) clearTimeout(searchTimeout)

    if (!appStore.activeExtId && query.startsWith('//')) {
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

    if (!appStore.activeExtId && query.startsWith('/')) {
      if (!wasToolListMode) {
        reset()
        selectedIndex.value = 0
      }
      results.value = buildToolListResults(query)
      if (selectedIndex.value >= results.value.length) selectedIndex.value = 0
      return
    }

    const searchId = ++currentSearchId

    if (appStore.activeExtId) {
      // 搜索型扩展（无 mainView、有 search）：标准列表走 searchEngine 扩展模式
      // mainView 扩展自管列表（resolvedView），无 search 的扩展无标准列表 → 均跳过
      const ext = activeExtension.value
      if (!ext || ext.mainView || !ext.search) return
      if (searchTimeout) clearTimeout(searchTimeout)
      searchTimeout = setTimeout(() => runExtensionSearch(query), 100)
      return
    }

    if (query.trim()) {
      // 全局搜索 30ms 防抖：应用缓存同步命中 + fieldScore 缓存使打分近乎即时，
      // searchEngine abort 机制保证竞态安全，文件搜索走 Rust 内存索引（~3ms）随打随出。
      // 30ms 合并快速连续按键，减少 ~60-80% 废弃搜索周期（打分/groupAndSort/对象分配），
      // 用户无感知延迟；扩展搜索保留 100ms 防抖（可能含 DB/网络慢查询）。
      searchTimeout = setTimeout(async () => {
        const finalResults = await searchEngine.search(query, (partial) => {
          if (searchId === currentSearchId) {
            results.value = partial
            clampSelected(partial.length)
          }
        })
        if (searchId === currentSearchId) {
          results.value = finalResults
          clampSelected(finalResults.length)
          // 注册本次结果对象的 GC 追踪（控制台 __mem() 手动查看回收情况）
          trackResults(query, finalResults)
        }
      }, 30)
    } else {
      await loadDefaultResults(true)
    }
  }

  // --- tag & focus ---

  const handleTagClose = () => {
    if (appStore.searchQuery) {
      clearSearch()
      if (appStore.activeExtId) {
        refreshExtension()
      } else {
        loadDefaultResults(true)
      }
    } else if (appStore.activeExtId) {
      exitExtension()
    }
    searchInput.value?.focus()
  }

  const focusHandler = async () => {
    if (activeExtension.value?.disableSearchInput) return
    searchInput.value?.focus()
    if (appStore.searchQuery) {
      searchInput.value?.select()
      // 隐藏时 results 已清空（释放 DOM），唤起重跑搜索恢复结果
      const ext = activeExtension.value
      if (appStore.activeExtId) {
        // 搜索型扩展重跑；mainView 扩展不走 results 无需处理
        if (ext && !ext.mainView && ext.search) runExtensionSearch(appStore.searchQuery)
      } else if (appStore.searchQuery.trim()) {
        runExtensionSearch(appStore.searchQuery)
      }
    } else if (!appStore.activeExtId) {
      await loadDefaultResults()
    }
  }

  /** 窗口唤起（主快捷键从隐藏呼出）时检查剪贴板：最新记录为文本且 3 秒内 → 填充搜索框。
   *  搜索框禁用（disableSearchInput 扩展激活）时跳过。 */
  async function maybeFillFromClipboard() {
    if (!isTauri) return
    if (activeExtension.value?.disableSearchInput) return
    try {
      // previewOnly 截断至 200 字符：搜索框不宜承载超长文本，避免模糊匹配 O(n×m) 开销
      const items = await invoke<
        Array<{ content: string; content_type: string; created_at: string }>
      >(CMD.getClipboardHistory, {
        filterFavorite: null,
        limit: 1,
        previewOnly: true,
      })
      if (items.length === 0) return
      const latest = items[0]
      if (latest.content_type !== 'text') return
      // created_at 为 SQLite UTC（YYYY-MM-DD HH:MM:SS），补 T+Z 解析为 UTC 毫秒时间戳
      const createdAt = new Date(latest.created_at.replace(' ', 'T') + 'Z').getTime()
      if (Date.now() - createdAt > 3000) return
      // 设值后派发 input 事件，复用 onInput 完整搜索链路（防抖/搜索引擎/结果更新）；
      // select 使后续输入直接替换填充内容（focusHandler 在 IPC 往返前已执行，此时 query 仍空不会 select）
      if (searchInput.value) {
        searchInput.value.value = latest.content
        searchInput.value.dispatchEvent(new Event('input', { bubbles: true }))
        searchInput.value.select()
      }
    } catch {
      // 剪贴板不可用时静默降级
    }
  }

  /** 窗口隐藏时取消进行中搜索与待触发的防抖：窗口不可见无需继续召回/打分。
   *  保留已渲染的 results：唤起时窗口立即可见上次结果（无空闪），focusHandler 再走应用缓存原地刷新。
   *  results 经 LIMITS 收紧后峰值约 44 节点，隐藏期间常驻开销可忽略（且下次搜索即整体替换，非泄漏源）。 */
  function onWindowHiding() {
    searchEngine.abort()
    if (searchTimeout) {
      clearTimeout(searchTimeout)
      searchTimeout = null
    }
  }

  onMounted(async () => {
    if (!activeExtension.value?.disableSearchInput) searchInput.value?.focus()
    await loadDefaultResults(true)
    // 启动基线：默认结果加载完成后的 JS 堆水位
    probeMem('boot')
    window.addEventListener('window-focused', focusHandler)
    window.addEventListener('window-invoked', maybeFillFromClipboard)
    window.addEventListener('window-hiding', onWindowHiding)
  })

  onUnmounted(() => {
    clearTimeout(iconTimer)
    if (searchTimeout) clearTimeout(searchTimeout)
    searchEngine.abort()
    window.removeEventListener('window-focused', focusHandler)
    window.removeEventListener('window-invoked', maybeFillFromClipboard)
    window.removeEventListener('window-hiding', onWindowHiding)
  })

  // 进入搜索型扩展（无 mainView、有 search）：触发初始 dynamic 装填结果
  watch(
    () => activeExtension.value?.meta.id,
    (newId, oldId) => {
      if (!newId || newId === oldId) return
      savedToolIndex = selectedIndex.value
      const ext = activeExtension.value
      if (!ext || ext.mainView || !ext.search) return
      // 立即清空旧结果 + 显示 loading：避免 HTTP 返回前残留全局列表/工具列表
      // （ContentView loading 占位条件 = loading && results.length === 0）
      results.value = []
      selectedIndex.value = 0
      isLoading.value = true
      runExtensionSearch(appStore.searchQuery)
    },
  )

  return {
    isLoading,
    onInput,
    clearSearch,
    loadDefaultResults,
    activateExtension,
    goHome,
    handleTagClose,
    refreshExtension,
    exitExtension,
  }
}
