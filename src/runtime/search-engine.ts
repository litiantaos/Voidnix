import { getAllExtensions } from './extension-registry'
import { SEARCH, LIMITS } from './constants'
import { scoreFields, keywordMatch } from '@/utils/fuzzy'
import type { SearchResult, SearchResultKind } from './types'

// kind → group 映射：file/folder 同属 'file' 组（v1.5 合并）；其余 kind 即组名。
function getGroupKey(kind: SearchResultKind | undefined): string {
  if (kind === 'file' || kind === 'folder') return 'file'
  return kind || 'other'
}

// 组间定序索引（值越小越靠前）
const GROUP_INDEX: Record<string, number> = SEARCH.GROUP_ORDER.reduce(
  (acc, key, idx) => {
    acc[key] = idx
    return acc
  },
  {} as Record<string, number>,
)

/** 单例搜索引擎：dynamic 单通道并行 + filter/group 管道（§2.5）。 */
class SearchEngine {
  private currentController?: AbortController
  private activeModule: string | undefined

  /** 模块激活/退出时切换模式。激活时只调该模块 dynamic；undefined 恢复全局聚合。 */
  setActiveModule(id: string | undefined) {
    this.activeModule = id
  }

  getActiveModule(): string | undefined {
    return this.activeModule
  }

  async search(query: string): Promise<SearchResult[]> {
    // 取消上一次查询（触发其 dynamic cleanup）
    this.currentController?.abort()
    const controller = new AbortController()
    this.currentController = controller

    // 1. dynamic 并行召回（框架按产出扩展 meta.id 注入 module）
    let results = await this.searchDynamic(query, controller.signal)

    // 1.5 keyword 合流（全局模式 only，模块模式禁用——已在某模块内不展示其他模块入口）。
    //    抑制 dynamic 已产出结果的扩展入口：即时答案优先（如「100 usd」已返回换算值，
    //    不再重复显示该扩展的模块入口）；dynamic 返空/失败时入口保留作降级。
    if (!this.activeModule && query.trim()) {
      const dynamicModules = new Set(results.map((r) => r.module))
      results = [
        ...results,
        ...this.keywordSearchAll(query).filter((r) => !dynamicModules.has(r.module)),
      ]
    }

    if (controller.signal.aborted) return []

    // 2. 去重（按 <module>:<id> 组合键）
    results = dedupe(results)

    // 3. 分组排序
    //    模块模式：保留扩展返回顺序（clipboard 时间序、calculator 即时结果优先等），
    //    仅去重不过滤不限流——模块内容是扩展自治的 UX。
    //    全局模式：groupAndSort（分组 + 零分过滤 + 组内限流）。
    if (this.activeModule) return results
    return this.groupAndSort(results, query)
  }

  /** 全局模式：并行调用所有扩展 dynamic；模块模式：只调 activeModule dynamic。
   *  每个扩展 dynamic 受 LIMITS.searchTimeoutMs 超时保护，慢扩展不拖住全局 Promise.all。 */
  private async searchDynamic(query: string, signal: AbortSignal): Promise<SearchResult[]> {
    const moduleMode = !!this.activeModule
    const exts = getAllExtensions().filter((e) => e.search)
    const targets = this.activeModule ? exts.filter((e) => e.meta.id === this.activeModule) : exts

    const settled = await Promise.all(
      targets.map(async (ext) => {
        try {
          const raw = await this.raceWithTimeout(ext.search!.dynamic(query, { signal, moduleMode }))
          // 框架注入 module = 产出扩展 meta.id（扩展禁填，§2.3 v1.6 N4）；
          // 全局模式 + 工具型结果（kind=module）注入 source = 扩展显示名（UI 标注来源，应用/文件等原生结果不注入）
          return raw.map(
            (r) =>
              ({
                ...r,
                module: ext.meta.id,
                ...(!moduleMode && r.data?.kind === 'module' ? { source: ext.meta.name } : {}),
              }) as SearchResult,
          )
        } catch (e) {
          // abort 触发的 AbortError 与超时是正常/降级路径，静默；其余打日志
          const name = (e as Error)?.name
          if (name !== 'AbortError' && name !== 'SearchTimeoutError') {
            console.error(`[search] extension '${ext.meta.id}' dynamic failed:`, e)
          }
          return []
        }
      }),
    )
    return settled.flat()
  }

  /** 为单个扩展 dynamic 套超时保护：超时抛 SearchTimeoutError（被调用方 catch 为降级 []）。
   *  dynamic 可同步返回数组或异步返回 Promise，Promise.resolve 统一包装。 */
  private async raceWithTimeout<T>(v: T | Promise<T>): Promise<T> {
    const p = Promise.resolve(v)
    let timer: ReturnType<typeof setTimeout> | undefined
    try {
      return await Promise.race([
        p,
        new Promise<T>((_, reject) => {
          timer = setTimeout(
            () =>
              reject(Object.assign(new Error('search timeout'), { name: 'SearchTimeoutError' })),
            LIMITS.searchTimeoutMs,
          )
        }),
      ])
    } finally {
      if (timer) clearTimeout(timer)
    }
  }

  /** 框架内置：扫描 meta.keywords 产出模块入口结果（§2.5）。
   *  keywords 用双向匹配（keywordMatch：正向 + 反向降权 + 拼音），覆盖多词 query 含关键词场景；
   *  name/description 用 scoreFields 单向子串（query 在 field 中）。 */
  private keywordSearchAll(query: string): SearchResult[] {
    const q = query.trim()
    if (!q) return []
    return getAllExtensions()
      .filter((e) => (e.meta.keywords?.length ?? 0) > 0)
      .map((ext) => {
        const score = Math.max(
          scoreFields([ext.meta.name, ext.meta.description], q),
          keywordMatch(ext.meta.keywords ?? [], q),
        )
        return { ext, score }
      })
      .filter((x) => x.score > 0)
      .sort((a, b) => b.score - a.score)
      .map(({ ext, score }) => ({
        id: `module-${ext.meta.id}`,
        title: ext.meta.name,
        description: ext.meta.description,
        icon: ext.meta.icon,
        module: ext.meta.id, // 目标模块 id（框架内置激活，§2.2 执行分派）
        boost: SEARCH.KEYWORD_MODULE_BOOST,
        score,
        data: { kind: 'module' as SearchResultKind, moduleId: ext.meta.id },
      }))
  }

  /** 管道：分组 → 组内 finalScore 降序 → 组间 GROUP_ORDER → 组内限流。
   *  全局模式专用（模块模式见 search() 直接返回）。 */
  private groupAndSort(items: SearchResult[], query: string): SearchResult[] {
    // 先算 finalScore = scoreFields(title[,description], query) + boost
    const scored = items
      .map((item) => {
        const fuzzy = scoreFields([item.title, item.description], query)
        const finalScore = fuzzy + (item.boost ?? 0)
        return { item, finalScore }
      })
      // 全局模式过滤零分（避免 calculator history 等无关项污染全局结果）
      .filter((x) => x.finalScore > 0)
      .map((x) => ({ ...x.item, score: x.finalScore }))

    // 分组
    const groups = new Map<string, SearchResult[]>()
    for (const item of scored) {
      const key = getGroupKey(item.data?.kind)
      if (!groups.has(key)) groups.set(key, [])
      groups.get(key)!.push(item)
    }

    // 组间按 GROUP_ORDER 定序
    const sortedGroups = [...groups.entries()].sort(
      (a, b) => (GROUP_INDEX[a[0]] ?? 99) - (GROUP_INDEX[b[0]] ?? 99),
    )

    const result: SearchResult[] = []
    for (const [key, groupItems] of sortedGroups) {
      // 组内 finalScore 降序
      groupItems.sort((a, b) => (b.score ?? 0) - (a.score ?? 0))
      // 组内限流
      const max = key === 'file' ? LIMITS.maxFileResults : LIMITS.maxAppResults
      result.push(...groupItems.slice(0, max))
    }
    return result
  }
}

/** 按 <module>:<id> 组合键去重（保留首个）。 */
function dedupe(items: SearchResult[]): SearchResult[] {
  const seen = new Set<string>()
  const out: SearchResult[] = []
  for (const item of items) {
    const key = `${item.module}:${item.id}`
    if (seen.has(key)) continue
    seen.add(key)
    out.push(item)
  }
  return out
}

export const searchEngine = new SearchEngine()
