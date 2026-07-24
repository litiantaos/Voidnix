import { getAllExtensions } from './extension-registry'
import { SEARCH, LIMITS } from './constants'
import { scoreFields, scoreModuleEntry } from '@/utils/fuzzy'
import type { Extension, SearchResult, SearchResultKind, ProviderResult } from './types'

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

/** 单例搜索引擎：流式增量召回（消除快结果等慢结果的 barrier）+ filter/group 管道。
 *  每个扩展 dynamic 的 emit/resolve 都触发一次增量重排并回调 onUpdate；快结果（应用缓存/同步扩展）
 *  秒出，慢结果（mdfind 文件/网络）增量补充，不再 Promise.all barrier。 */
class SearchEngine {
  private currentController?: AbortController
  private activeModule: string | undefined
  // keyword 入口记忆化：同 query 的 keyword 结果不变，增量 flush 复用避免重算
  private kwCacheQ: string | null = null
  private kwCache: SearchResult[] = []

  /** 模块激活/退出时切换模式。激活时只调该模块 dynamic；undefined 恢复全局聚合。 */
  setActiveModule(id: string | undefined) {
    this.activeModule = id
  }

  /** 取消进行中的 search（模块退出 / 组件卸载）。新 search() 也会 abort 上一次。 */
  abort() {
    this.currentController?.abort()
    this.currentController = undefined
  }

  /** 流式搜索：query 为当前输入；onUpdate 在每次有新结果（扩展 emit/resolve）时回调增量重排结果。
   *  返回 Promise 解析为最终完整结果（与最后一次 onUpdate 一致）。不传 onUpdate 时退化为一次性返回。
   *  取消上一次查询（触发其 dynamic cleanup + child abort）。 */
  async search(
    query: string,
    onUpdate?: (results: SearchResult[]) => void,
  ): Promise<SearchResult[]> {
    this.currentController?.abort()
    const controller = new AbortController()
    this.currentController = controller

    // 失效 keyword 缓存：跨 search() 调用时扩展注册表可能已变化（测试场景 / 运行时动态注册）
    this.kwCacheQ = null

    // 快照模式：await 期间 activeModule 可能被 KeepAlive/快捷键改写，后处理必须与本次召回一致
    const moduleId = this.activeModule
    const moduleMode = !!moduleId
    const q = query.trim()

    if (moduleMode) {
      // 模块模式：累积 raw 结果，每次扩展 emit/resolve 都 dedupe + onUpdate（保留扩展返回序，不过滤不限流）
      const acc: SearchResult[] = []
      let last: SearchResult[] | undefined // 缓存最近一次 flush 结果，return 复用避免重复 dedupe
      const flush = () => {
        if (!controller.signal.aborted) {
          last = dedupeBy(acc, (r) => `${r.module}:${r.id}`)
          onUpdate?.(last)
        }
      }
      await this.collectAll(query, controller.signal, moduleId, moduleMode, (items) => {
        acc.push(...items)
        flush()
      })
      // last 有值 = flush 至少执行过一次，最后一次与 return 等价直接复用；无值（无扩展产出/已 abort）补算
      return last ?? dedupeBy(acc, (r) => `${r.module}:${r.id}`)
    }

    // 全局模式：累积 ScoredResult（打分只算一次），每次扩展 emit/resolve 都 keyword 合流 + groupAndSort + onUpdate
    const scored: ScoredResult[] = []
    let last: SearchResult[] | undefined // 缓存最近一次 flush 结果，return 复用避免重复 buildGlobal
    const flush = () => {
      if (!controller.signal.aborted) {
        last = this.buildGlobal(scored, q)
        onUpdate?.(last)
      }
    }
    await this.collectAll(query, controller.signal, moduleId, moduleMode, (items) => {
      scored.push(...this.scoreResults(items, q))
      flush()
    })
    return last ?? this.buildGlobal(scored, q)
  }

  /** 并发启动所有目标扩展的 dynamic；每个扩展的 emit（部分结果）与 resolve（最终/补充结果）
   *  都经 annotate 后回调 onBatch。用 Promise.allSettled 等待全部结束（但 onUpdate 已沿途增量触发，
   *  快结果无需等慢结果）。每扩展 dynamic 受 LIMITS.searchTimeoutMs 超时保护，慢扩展不牵连其它；
   *  父 signal abort 时同步取消 child。 */
  private async collectAll(
    query: string,
    signal: AbortSignal,
    moduleId: string | undefined,
    moduleMode: boolean,
    onBatch: (items: SearchResult[]) => void,
  ): Promise<void> {
    const exts = getAllExtensions().filter((e) => e.search)
    const targets = moduleId ? exts.filter((e) => e.meta.id === moduleId) : exts

    // 空批次跳过：扩展 emit 后 return [] 等场景不触发多余的重排 + 渲染
    const batch = (items: SearchResult[]) => {
      if (items.length > 0) onBatch(items)
    }

    await Promise.allSettled(
      targets.map(async (ext) => {
        const child = new AbortController()
        const onParentAbort = () => child.abort()
        if (signal.aborted) {
          child.abort()
        } else {
          signal.addEventListener('abort', onParentAbort, { once: true })
        }
        // emit 绑定到本次 search 的累积器：扩展流式产出的部分结果立即增量重排
        const emit = (partial: ProviderResult[]) => {
          if (signal.aborted) return
          batch(this.annotate(partial, ext, moduleMode))
        }
        try {
          const raw = await this.raceWithTimeout(
            ext.search!.dynamic(query, { signal: child.signal, moduleMode, emit }),
            () => child.abort(),
          )
          if (!signal.aborted) batch(this.annotate(raw, ext, moduleMode))
        } catch (e) {
          // abort 触发的 AbortError 与超时是正常/降级路径，静默；其余打日志
          const name = (e as Error)?.name
          if (name !== 'AbortError' && name !== 'SearchTimeoutError') {
            console.error(`[search] extension '${ext.meta.id}' dynamic failed:`, e)
          }
        } finally {
          signal.removeEventListener('abort', onParentAbort)
        }
      }),
    )
  }

  /** 框架注入：module = 产出扩展 meta.id（扩展禁填）；
   *  module 类动态结果无 icon 时补产出扩展 meta.icon（calculator/currency 等即时答案默认带扩展图标）；
   *  全局模式 + 工具型结果（kind=module）注入 source = 扩展显示名（UI 标注来源，应用/文件等原生结果不注入）。 */
  private annotate(raw: ProviderResult[], ext: Extension, moduleMode: boolean): SearchResult[] {
    return raw.map((r) => {
      const isModule = r.data?.kind === 'module'
      return {
        ...r,
        module: ext.meta.id,
        icon:
          r.icon ?? (r.data?.icon as string | undefined) ?? (isModule ? ext.meta.icon : undefined),
        ...(!moduleMode && isModule ? { source: ext.meta.name } : {}),
      } as SearchResult
    })
  }

  /** 一次预算 finalScore（suppress 判断 + groupAndSort 复用，消除二次 scoreFields）。
   *  matched：空 query 需 finalScore>0（boost>0）；非空 query 需 fuzzy>0（查找型必须命中）。 */
  private scoreResults(items: SearchResult[], q: string): ScoredResult[] {
    return items.map((r) => {
      const boost = r.boost ?? 0
      const fuzzy = q ? scoreFields([r.title, r.description], q) : 0
      const finalScore = fuzzy + boost
      const matched = q ? fuzzy > 0 : finalScore > 0
      return { item: r, finalScore, matched }
    })
  }

  /** 全局模式最终管道：keyword 合流（过滤已被 dynamic module 结果覆盖的扩展入口）+ 去重 + groupAndSort。
   *  每次 flush（增量）与最终返回共用——保证增量与最终结果一致。keyword 纯同步且扩展数少，每次重算可接受。 */
  private buildGlobal(scored: ScoredResult[], q: string): SearchResult[] {
    let all = scored
    if (q) {
      // 只在 dynamic 产出相关 tool 型结果（kind=module，finalScore > 0）时抑制该扩展入口：
      // 即时答案优先（如「100 usd」返回换算值不再与模块入口同屏）；
      // clipboard 等数据型结果（kind≠module）不抑制——用户搜「剪贴板」时先看模块入口再看记录。
      const relevantDynamicModules = new Set(
        scored
          .filter((x) => x.item.data?.kind === 'module' && x.finalScore > 0)
          .map((x) => x.item.module),
      )
      const kwScored = this.keywordSearchAll(q)
        .filter((r) => !relevantDynamicModules.has(r.module))
        .map((r) => ({ item: r, finalScore: (r.score ?? 0) + (r.boost ?? 0), matched: true }))
      all = [...scored, ...kwScored]
    }

    const deduped = dedupeBy(all, (x) => `${x.item.module}:${x.item.id}`)
    return this.groupAndSort(deduped)
  }

  /** 为单个扩展 dynamic 套超时保护：超时抛 SearchTimeoutError（被调用方 catch 为降级 []）。
   *  超时立即 onTimeout（abort 该扩展 child signal），停止 in-flight HTTP/IPC。
   *  dynamic 可同步返回数组或异步返回 Promise，Promise.resolve 统一包装。 */
  private async raceWithTimeout<T>(v: T | Promise<T>, onTimeout: () => void): Promise<T> {
    const p = Promise.resolve(v)
    let timer: ReturnType<typeof setTimeout> | undefined
    try {
      return await Promise.race([
        p,
        new Promise<T>((_, reject) => {
          timer = setTimeout(() => {
            onTimeout()
            reject(Object.assign(new Error('search timeout'), { name: 'SearchTimeoutError' }))
          }, LIMITS.searchTimeoutMs)
        }),
      ])
    } finally {
      if (timer) clearTimeout(timer)
    }
  }

  /** 框架内置：scoreModuleEntry 产出模块入口（与 `/` 工具列表共用打分）。
   *  入参 q 约定已 trim；产出序无要求——groupAndSort 在 module 组内按 finalScore 重排。
   *  按 q 记忆化：同 query 的 keyword 入口不变，增量 flush 直接复用。 */
  private keywordSearchAll(q: string): SearchResult[] {
    if (this.kwCacheQ === q) return this.kwCache
    this.kwCacheQ = q
    this.kwCache = getAllExtensions()
      .map((ext) => ({ ext, score: scoreModuleEntry(ext.meta, q) }))
      .filter((x) => x.score > 0)
      .map(({ ext, score }) => ({
        id: `module-${ext.meta.id}`,
        title: ext.meta.name,
        description: ext.meta.description,
        icon: ext.meta.icon,
        module: ext.meta.id,
        boost: SEARCH.KEYWORD_MODULE_BOOST,
        score,
        data: { kind: 'module' as SearchResultKind, moduleId: ext.meta.id },
      }))
    return this.kwCache
  }

  /** 管道：过滤 → 回填 score → 分组 → 组内 finalScore 降序 → 组间 GROUP_ORDER → 组内限流。
   *  全局模式专用（模块模式见 search() 直接返回）。复用 ScoredResult.finalScore，不再调 scoreFields。
   *  过滤：matched（query 命中或空 query）保留；非 matched 仅 module 类即时答案靠 finalScore>0 穿透。
   *  单次遍历合并 filter + score 回填 + 分组，消除两次中间数组分配。 */
  private groupAndSort(items: ScoredResult[]): SearchResult[] {
    const groups = new Map<string, SearchResult[]>()
    for (const x of items) {
      if (!x.matched && !(x.item.data?.kind === 'module' && x.finalScore > 0)) continue
      const item: SearchResult = { ...x.item, score: x.finalScore }
      const key = getGroupKey(item)
      let group = groups.get(key)
      if (!group) {
        group = []
        groups.set(key, group)
      }
      group.push(item)
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
