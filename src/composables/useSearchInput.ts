import { ref, watch, type Ref, type ComputedRef, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useTauriListener } from '@/composables/useTauriListener'
import { searchEngine, getGroupKey } from '@/runtime/search-engine'
import { getAllExtensions } from '@/runtime/extension-registry'
import { scoreExtensionEntry } from '@/utils/fuzzy'
import { resolveLocalized, t } from '@/runtime/i18n'
import { useAppStore } from '@/stores/app'
import { CMD } from '@/commands'
import { SEARCH } from '@/runtime/constants'
import type { Extension, SearchResult } from '@/runtime/types'
import { isTauri } from '@/utils/tauri'
import { buildOpenUrlResult, buildWebSearchResult, parseWebSearchQuery } from '@/utils/web-search'
import { probeMem, trackResults } from '@/utils/mem-probe'
import { isModalDialogOpen } from '@/utils/dom'

interface SearchInputOptions {
  searchInput: Ref<HTMLInputElement | undefined>
  results: Ref<SearchResult[]>
  selectedIndex: Ref<number>
  activeExtension: ComputedRef<Extension | null>
  reset: () => void
}

/** 全选但不触发系统选中动画：macOS 26 WKWebView 对聚焦元素的选区变更播放选中动画
 *  （从当前光标位展开，唤起场景锚点在末端 → 蓝色从右到左扫过）。先 blur 使选区变更
 *  落在元素未聚焦态（不动画），同步设全选后回焦；三步同一任务内完成，下一帧直接
 *  以最终态（聚焦蓝全选）呈现。 */
function selectAllWithoutAnimation(el: HTMLInputElement) {
  el.blur()
  el.setSelectionRange(0, el.value.length)
  el.focus()
}

/** 同 query 刷新的稳定合并：内容取 next（新对象/新数据），顺序以 prev 为基——
 *  本次搜索的排序已定，usage/recency 加权变化不改当前会话排序（打开条目触发的
 *  increment_use_count 升位在下次输入的新搜索生效），唤起时零重排、选中零跳位。
 *  新条目插到同组末尾（组头按相邻组值渲染，天然连续），无同组项追加尾部。
 *  dropMissing：final 精确对齐（next 缺席 = 已消失，移除）；增量 partial 的缺席
 *  = 未到达，prev 条目保留（防子集 partial 收缩列表引发闪烁）。 */
function stableMerge(
  prev: SearchResult[],
  next: SearchResult[],
  dropMissing: boolean,
): SearchResult[] {
  const key = (r: SearchResult) => `${r.extId}:${r.id}`
  const nextMap = new Map(next.map((r) => [key(r), r]))
  const known = new Set(prev.map(key))
  const merged = prev
    .filter((r) => !dropMissing || nextMap.has(key(r)))
    .map((r) => nextMap.get(key(r)) ?? r)
  for (const item of next) {
    if (known.has(key(item))) continue
    const group = getGroupKey(item)
    let at = -1
    for (let i = merged.length - 1; i >= 0; i--) {
      if (getGroupKey(merged[i]) === group) {
        at = i + 1
        break
      }
    }
    if (at < 0) at = merged.length
    merged.splice(at, 0, item)
    known.add(key(item))
  }
  return merged
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
      title: resolveLocalized(ext.meta.name),
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

  /** 空查询默认列表尾部固定提示行：引导发现 `/` 工具列表（kind=extension 入扩展组，
   *  无 extId 不走扩展激活；回车分派见 useResultNavigation）。 */
  function toolsHintResult(): SearchResult {
    return {
      id: SEARCH.TOOLS_HINT_ID,
      title: t('search.browseToolsHint'),
      icon: 'i-ri-apps-2-line',
      extId: 'voidnix',
      data: { kind: 'extension' },
    }
  }

  /** 打开 `/` 工具列表（提示行回车入口）：等价于输入 / 的完整链路。 */
  function openToolList() {
    ++currentSearchId
    searchEngine.abort()
    if (searchTimeout) clearTimeout(searchTimeout)
    clearSearch('/')
    results.value = buildToolListResults('/')
    selectedIndex.value = 0
    searchInput.value?.focus()
    searchInput.value?.select()
  }

  async function loadDefaultResults(resetSelection = false) {
    if (!isTauri) return
    const searchId = ++currentSearchId
    // 转移入口（退出扩展/回主页/清空输入）显式归首项、规范序；后台刷新（图标就绪/缓存变更/
    // 窗口获焦）同 query 稳定合并（旧序为基 + 选中身份跟随），usage/recency 加权变化
    // 不改当前会话排序，唤起时零重排零跳位。
    if (resetSelection) {
      // 同步落提示行：默认列表异步到达前的窗口期（IPC + 缓存扫描约 10-50ms）内，残留的
      // 上一会话结果仍占据列表且可被回车执行——退出扩展/goHome 后紧接的回车会把残留
      // 剪贴板记录直接粘贴出去（无 toast 即关窗）。同步替换杜绝该窗口（与下方 catch
      // 分支的兜底同款单行，无空态闪烁）。
      results.value = [toolsHintResult()]
      selectedIndex.value = 0
    }
    const prevList = resetSelection ? [] : results.value
    const prevSel = resetSelection ? undefined : results.value[selectedIndex.value]
    // 尾部固定提示行：随每次默认列表刷新（增量与最终）追加
    const withHint = (list: SearchResult[]) => [...list, toolsHintResult()]
    const apply = (list: SearchResult[], final: boolean) => {
      const applied = resetSelection ? list : stableMerge(prevList, list, final)
      results.value = applied
      applyListWithSelection(applied, prevSel)
    }
    try {
      const defaultResults = await searchEngine.search('', (partial) => {
        if (searchId === currentSearchId) apply(withHint(partial), false)
      })
      if (searchId === currentSearchId) apply(withHint(defaultResults), true)
    } catch {
      if (searchId === currentSearchId) {
        // 后台刷新失败保留现有列表（不闪空态）；转移入口（resetSelection）或本就为空
        // 才落提示行单行兜底
        if (resetSelection || results.value.length === 0) {
          results.value = [toolsHintResult()]
          selectedIndex.value = 0
        }
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

  /** 后台刷新应用新列表时的选中策略：按条目身份（extId+id）跟随而非钉死索引。
   *  稳定合并下条目不动、选中天然不动；此策略兜底移除场景（上方条目消失使索引前移错位）。
   *  条目不在新列表（增量未到/已消失）时仅 clamp 越界，索引原地等后续增量补到再跟随。 */
  function applyListWithSelection(list: SearchResult[], prev?: SearchResult) {
    if (prev) {
      const idx = list.findIndex((r) => r.id === prev.id && r.extId === prev.extId)
      if (idx >= 0) {
        selectedIndex.value = idx
        return
      }
    }
    clampSelected(list.length)
  }

  /** 焦点重跑（窗口重新唤起刷新数据）：不传 onUpdate —— 增量 partial 会把已显示的完整列表
   *  先替换为较短中间态（应用缓存先 flush、文件索引后至），文件组瞬间消失再恢复：
   *  列表闪烁 + scrollTop 随内容收缩被 clamp + 选中越界被 clampSelected 归零。
   *  静默重跑仅在最终结果就绪时经 stableMerge 稳定合并一次性应用：内容刷新（新缓存/剪贴板/
   *  文件）、顺序保持隐藏前列表（query 不变排序不变，usage/recency 升位等下次输入的新搜索
   *  生效），行 DOM 按 id 复用零重排零跳位；上次结果为空（搜索失败/中断）退回流式 + loading 占位。 */
  async function rerunSearch(query: string) {
    if (results.value.length === 0) {
      runExtensionSearch(query)
      return
    }
    const searchId = ++currentSearchId
    // 刷新语义要求数据新鲜：清结果缓存防命中（onWindowHiding 已清，此处兜底 blur 未藏窗的获焦刷新）
    searchEngine.clearResultCache()
    // 身份捕获于 await 前：await 期间用户输入会开新搜索（searchId 守卫），prev 不受影响
    const prevList = results.value
    const prevSel = prevList[selectedIndex.value]
    try {
      const res = await searchEngine.search(query)
      if (searchId === currentSearchId) {
        const merged = stableMerge(prevList, res, true)
        results.value = merged
        applyListWithSelection(merged, prevSel)
      }
    } catch {
      // 重跑失败保留现有结果（不空屏）
    }
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
        // partial 经 stableMerge 稳定合并（追加不重排，与 rerunSearch 同语义）：跨过引擎
        // 首帧合批窗口的错峰到达（应用缓存冷重建、高负载下文件索引 IPC 超 50ms）不再
        // 「顶部插入 + 整表重排」跳动，新条目落同组尾部；prev 条目保留至 final 规范序
        // 一次性替换（缺席 = 未到达，防收缩闪烁）。合并不前插，选中索引天然稳定。
        const prevList = results.value
        const finalResults = await searchEngine.search(query, (partial) => {
          if (searchId === currentSearchId) {
            const merged = stableMerge(prevList, partial, false)
            results.value = merged
            clampSelected(merged.length)
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

  /** 唤起路径的输入框全选。三重时序约束（macOS 26 WKWebView，show 不 activate_app）：
   *  1) 页面获焦前 select：以非聚焦选中色（灰）绘制，获焦后翻系统蓝——灰→蓝跳变；
   *  2) 页面获焦后对聚焦元素 select：系统选中动画从当前光标位展开（锚点=隐藏时折叠的
   *     末端 → 蓝色从右到左扫过）；
   *  3) 原生 focus 事件在无激活唤起下迟发/被 show 过程的瞬时页面 blur 打断（实测约 2s
   *     后由激活链路稳态的二次 onFocusChanged 补发）——不能只等事件。
   *  解法：hasFocus 已真立即三步落位；未真则「原生 focus 事件 + rAF 轮询 hasFocus 状态」
   *  双通道探测，状态翻转即落位（键盘输入可达 = 状态已翻转，事件迟发不影响）。落位一律经
   *  selectAllWithoutAnimation（blur → 全选 → focus 同任务三步）：选区变更落在元素未聚焦
   *  态不触发动画，下一帧直接以聚焦蓝完整呈现。清理只由 window-hiding / 卸载 / 下次调用
   *  触发，瞬时页面 blur 不取消（否则待定全选被杀、迟至二次触发才选中）。 */
  let cancelPendingSelect: (() => void) | null = null
  function selectAllWhenPageFocused() {
    cancelPendingSelect?.()
    const el = searchInput.value
    if (!el) return
    if (document.hasFocus()) {
      selectAllWithoutAnimation(el)
      return
    }
    let settled = false
    let rafId: number | null = null
    const cleanup = () => {
      window.removeEventListener('focus', onFocus)
      if (rafId !== null) cancelAnimationFrame(rafId)
      cancelPendingSelect = null
    }
    const settle = () => {
      if (settled) return
      settled = true
      cleanup()
      selectAllWithoutAnimation(el)
    }
    const onFocus = () => settle()
    const poll = () => {
      if (settled) return
      if (document.hasFocus()) return settle()
      rafId = requestAnimationFrame(poll)
    }
    cancelPendingSelect = cleanup
    window.addEventListener('focus', onFocus)
    rafId = requestAnimationFrame(poll)
  }

  const focusHandler = async () => {
    if (activeExtension.value?.disableSearchInput) return
    // 模态弹窗打开（如菜单栏「检查更新」唤起同时弹 UpdateDialog）：焦点归弹窗
    //（mount 时已落位按钮），不抢搜索框、不启动唤起全选轮询；数据刷新照旧
    const modalOpen = isModalDialogOpen()
    if (!modalOpen) searchInput.value?.focus()
    if (appStore.searchQuery) {
      if (!modalOpen) selectAllWhenPageFocused()
      // 重跑搜索刷新数据（results 虽保留可见，但隐藏期间可能有新缓存/剪贴板记录）。
      // 一律走 rerunSearch 静默重跑：无增量 partial 替换，唤起时滚动/选中与隐藏前一致
      const ext = activeExtension.value
      if (appStore.activeExtId) {
        // 搜索型扩展重跑；mainView 扩展不走 results 无需处理
        if (ext && !ext.mainView && ext.search) rerunSearch(appStore.searchQuery)
      } else if (!appStore.searchQuery.startsWith('/')) {
        // 全局搜索重跑。工具列表（/）/ 网页搜索（//）结果由 query 确定性生成，
        // 隐藏期间不会过期且 DOM 已保留，走全局搜索会把工具列表替换成应用搜索结果
        if (appStore.searchQuery.trim()) rerunSearch(appStore.searchQuery)
      }
    } else if (!appStore.activeExtId) {
      await loadDefaultResults()
    }
  }

  /** 窗口唤起（主快捷键从隐藏呼出）时检查剪贴板：最新记录为文本且 3 秒内 → 填充搜索框。
   *  仅主界面填充（「快速搜刚复制内容」特性，query 变化属内容切换）；整窗视图接管 /
   *  扩展激活时跳过——扩展内 query 是列表过滤参数，填充会破坏浏览上下文（剪贴板等
   *  用搜索框过滤的扩展），窗口显隐不得改变扩展内容状态。 */
  async function maybeFillFromClipboard() {
    if (!isTauri) return
    if (appStore.fullscreenView) return
    if (appStore.activeExtId) return
    // 模态弹窗打开：不填充（query 不被污染）、不启动会抢焦点全选轮询
    if (isModalDialogOpen()) return
    if (!searchInput.value) return
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
      // select 使后续输入直接替换填充内容（focusHandler 在 IPC 往返前已执行，此时 query 仍空不会 select）。
      // 同样经 selectAllWhenPageFocused：window-invoked 与 window-focused 同源先于页面焦点翻转
      if (searchInput.value) {
        searchInput.value.value = latest.content
        searchInput.value.dispatchEvent(new Event('input', { bubbles: true }))
        selectAllWhenPageFocused()
      }
    } catch {
      // 剪贴板不可用时静默降级
    }
  }

  /** 窗口隐藏时取消进行中的搜索 + 清理防抖定时器。
   *  不清空 results：主快捷键由 Rust 直接 show 窗口（前端 IPC 回调在 show 之后），
   *  若 results 已清空则第一帧渲染空态，待 loadDefaultResults 异步完成才出列表——产生闪烁。
   *  保留 DOM 使唤起时列表立即可见，focusHandler 后台刷新补增量。
   *  输入框折叠残留选中并取消待定全选：唤起首帧页面焦点未落定，残留选中以非聚焦灰绘制、
   *  获焦后翻蓝（灰→蓝跳变）；折叠后唤起由 selectAllWhenPageFocused 在获焦时一次到位。
   *  compositing layer 释放由 ContentView.clearCache 统一承担：content-visibility:hidden
   *  跳过子树渲染并释放 tile backing，结果列表与扩展视图的 DOM/状态冻结保留。 */
  function onWindowHiding() {
    searchEngine.abort()
    // 会话级结果缓存随会话结束清空：隐藏期间剪贴板/应用缓存可能变更
    searchEngine.clearResultCache()
    if (searchTimeout) {
      clearTimeout(searchTimeout)
      searchTimeout = null
    }
    cancelPendingSelect?.()
    cancelPendingSelect = null
    const el = searchInput.value
    if (el && el.selectionStart !== el.selectionEnd) {
      const end = el.value.length
      el.setSelectionRange(end, end)
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
    cancelPendingSelect?.()
    cancelPendingSelect = null
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
    openToolList,
  }
}
