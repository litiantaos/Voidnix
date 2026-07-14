import { getAllExtensions } from './extension-registry'
import { SEARCH, LIMITS } from './constants'
import { scoreFields, scoreModuleEntry } from '@/utils/fuzzy'
import type { SearchResult, SearchResultKind } from './types'

// kind → group 映射：file/folder 同属 'file' 组；其余 kind 即组名。
// 框架级列表分组单一源（ContentView 全局模式 + search-engine groupAndSort 复用）。
export function getGroupKey(item: SearchResult): string {
  const kind = item.data?.kind
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

// 打分后中间结构：finalScore 一次预算，suppress 判断 + groupAndSort 复用（消除二次 scoreFields）
// matched = 应显示：空 query 需 finalScore>0（boost>0，默认列表过滤 time/uuid 等 boost=0 即时答案）；
// 非空 query 需 fuzzy>0（查找型结果必须命中）。非 matched 时仅 module 类即时答案可靠 finalScore>0 穿透
interface ScoredResult {
  item: SearchResult
  finalScore: number
  matched: boolean
}

/** 单例搜索引擎：dynamic 单通道并行 + filter/group 管道。 */
class SearchEngine {
  private currentController?: AbortController
  private activeModule: string | undefined

  /** 模块激活/退出时切换模式。激活时只调该模块 dynamic；undefined 恢复全局聚合。 */
  setActiveModule(id: string | undefined) {
    this.activeModule = id
  }

  /** 取消进行中的 search（模块退出 / 组件卸载）。新 search() 也会 abort 上一次。 */
  abort() {
    this.currentController?.abort()
    this.currentController = undefined
  }

  async search(query: string): Promise<SearchResult[]> {
    // 取消上一次查询（触发其 dynamic cleanup）
    this.currentController?.abort()
    const controller = new AbortController()
    this.currentController = controller

    // 1. dynamic 并行召回（框架按产出扩展 meta.id 注入 module）
    const results = await this.searchDynamic(query, controller.signal)
    if (controller.signal.aborted) return []

    // 模块模式短路：仅去重，保留扩展返回序（不过滤不限流——模块内容是扩展自治 UX），跳过打分预算
    if (this.activeModule) return dedupeBy(results, (r) => `${r.module}:${r.id}`)

    const q = query.trim()

    // 2. 一次预算 dynamic 部分的 finalScore（suppress 判断 + groupAndSort 复用，消除二次 scoreFields）
    //    matched：空 query 默认列表需 finalScore>0（即 boost>0，主要是应用启动屏，过滤 time/uuid 等 boost=0 的即时答案）；
    //    非空 query 需 fuzzy>0（应用/文件等查找型结果必须命中）。
    //    module 类即时答案（calculator/currency 等）靠 boost 穿透——title 不含 query 也能展示。
    const scored: ScoredResult[] = results.map((r) => {
      const boost = r.boost ?? 0
      const fuzzy = q ? scoreFields([r.title, r.description], q) : 0
      const finalScore = fuzzy + boost
      const matched = q ? fuzzy > 0 : finalScore > 0
      return { item: r, finalScore, matched }
    })

    // 3. keyword 合流（全局模式 only，模块模式禁用——已在某模块内不展示其他模块入口）。
    //    只在 dynamic 产出相关 tool 型结果（kind=module，finalScore > 0）时抑制该扩展入口：
    //    即时答案优先（如「100 usd」返回换算值不再与模块入口同屏）；
    //    clipboard 等数据型结果（kind≠module）不抑制——用户搜「剪贴板」时先看模块入口再看记录。
    //    keyword 入口 finalScore 复用 keywordSearchAll 内部 score（含 keywordMatch 反向匹配贡献）。
    if (q) {
      const relevantDynamicModules = new Set(
        scored
          .filter((x) => x.item.data?.kind === 'module' && x.finalScore > 0)
          .map((x) => x.item.module),
      )
      const kwScored = this.keywordSearchAll(q)
        .filter((r) => !relevantDynamicModules.has(r.module))
        .map((r) => ({ item: r, finalScore: (r.score ?? 0) + (r.boost ?? 0), matched: true }))
      scored.push(...kwScored)
    }

    // 4. 去重（按 <module>:<id> 组合键）
    const deduped = dedupeBy(scored, (x) => `${x.item.module}:${x.item.id}`)

    // 5. 分组排序
    //    模块模式已在上方短路返回；全局模式 groupAndSort（分组 + 零分过滤 + 组内限流），复用 finalScore 不再重算。
    return this.groupAndSort(deduped)
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
          // 框架注入 module = 产出扩展 meta.id（扩展禁填）；
          // module 类动态结果无 icon 时补产出扩展 meta.icon（calculator/currency 等即时答案默认带扩展图标）；
          // 全局模式 + 工具型结果（kind=module）注入 source = 扩展显示名（UI 标注来源，应用/文件等原生结果不注入）
          return raw.map((r) => {
            const isModule = r.data?.kind === 'module'
            return {
              ...r,
              module: ext.meta.id,
              icon:
                r.icon ??
                (r.data?.icon as string | undefined) ??
                (isModule ? ext.meta.icon : undefined),
              ...(!moduleMode && isModule ? { source: ext.meta.name } : {}),
            } as SearchResult
          })
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

  /** 框架内置：scoreModuleEntry 产出模块入口（与 `/` 工具列表共用打分）。
   *  入参 q 约定已 trim；产出序无要求——groupAndSort 在 module 组内按 finalScore 重排。 */
  private keywordSearchAll(q: string): SearchResult[] {
    return getAllExtensions()
      .map((ext) => ({ ext, score: scoreModuleEntry(ext.meta, q) }))
      .filter((x) => x.score > 0)
      .map(({ ext, score }) => ({
        id: `module-${ext.meta.id}`,
        title: ext.meta.name,
        description: ext.meta.description,
        icon: ext.meta.icon,
        module: ext.meta.id, // 目标模块 id（框架内置激活）
        boost: SEARCH.KEYWORD_MODULE_BOOST,
        score,
        data: { kind: 'module' as SearchResultKind, moduleId: ext.meta.id },
      }))
  }

  /** 管道：分组 → 组内 finalScore 降序 → 组间 GROUP_ORDER → 组内限流。
   *  全局模式专用（模块模式见 search() 直接返回）。复用 ScoredResult.finalScore，不再调 scoreFields。
   *  过滤：matched（query 命中或空 query）保留；非 matched 仅 module 类即时答案靠 finalScore>0 穿透。 */
  private groupAndSort(items: ScoredResult[]): SearchResult[] {
    // 过滤 + 回填 score（UI/调试可读）
    const filtered = items
      .filter((x) => x.matched || (x.item.data?.kind === 'module' && x.finalScore > 0))
      .map((x) => ({ ...x.item, score: x.finalScore }))

    // 分组
    const groups = new Map<string, SearchResult[]>()
    for (const item of filtered) {
      const key = getGroupKey(item)
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
      const max = key === 'file' ? LIMITS.maxFileResults : LIMITS.maxGroupResults
      result.push(...groupItems.slice(0, max))
    }
    return result
  }
}

/** 按自定义 key 去重（保留首个）。模块模式对 SearchResult、全局模式对 ScoredResult 复用同一机制。 */
function dedupeBy<T>(items: T[], keyFn: (x: T) => string): T[] {
  const seen = new Set<string>()
  const out: T[] = []
  for (const x of items) {
    const key = keyFn(x)
    if (seen.has(key)) continue
    seen.add(key)
    out.push(x)
  }
  return out
}

export const searchEngine = new SearchEngine()
