import { getAllExtensions } from './extension-registry'
import { SEARCH, LIMITS } from './constants'
import { scoreFields } from '@/utils/fuzzy'
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

    // 1.5 keyword 合流（全局模式 only，模块模式禁用——已在某模块内不展示其他模块入口）
    if (!this.activeModule && query.trim()) {
      results = [...results, ...this.keywordSearchAll(query)]
    }

    if (controller.signal.aborted) return []

    // 2. 去重（按 <module>:<id> 组合键）
    results = dedupe(results)

    // 3. 分组排序
    results = this.groupAndSort(results, query)

    return results
  }

  /** 全局模式：并行调用所有扩展 dynamic；模块模式：只调 activeModule dynamic。 */
  private async searchDynamic(query: string, signal: AbortSignal): Promise<SearchResult[]> {
    const exts = getAllExtensions().filter((e) => e.search)
    const targets = this.activeModule
      ? exts.filter((e) => e.meta.id === this.activeModule)
      : exts

    const settled = await Promise.all(
      targets.map(async (ext) => {
        try {
          const raw = await ext.search!.dynamic(query, { signal })
          // 框架注入 module = 产出扩展 meta.id（扩展禁填，§2.3 v1.6 N4）
          return raw.map((r) => ({ ...r, module: ext.meta.id }) as SearchResult)
        } catch (e) {
          // abort 触发的 AbortError 是正常路径，静默；其余打日志
          if ((e as Error)?.name !== 'AbortError') {
            console.error(`[search] extension '${ext.meta.id}' dynamic failed:`, e)
          }
          return []
        }
      }),
    )
    return settled.flat()
  }

  /** 框架内置：扫描 meta.keywords 产出模块入口结果（§2.5）。 */
  private keywordSearchAll(query: string): SearchResult[] {
    const q = query.trim()
    if (!q) return []
    return getAllExtensions()
      .filter((e) => (e.meta.keywords?.length ?? 0) > 0)
      .map((ext) => {
        const score = scoreFields(
          [ext.meta.name, ext.meta.description, ...(ext.meta.keywords ?? [])],
          q,
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

  /** 管道：分组 → 组内 finalScore 降序 → 组间 GROUP_ORDER → 组内限流。 */
  private groupAndSort(items: SearchResult[], query: string): SearchResult[] {
    // 先算 finalScore = scoreFields(title[,description], query) + boost
    const scored = items.map((item) => {
      const fuzzy = scoreFields([item.title, item.description], query)
      const finalScore = fuzzy + (item.boost ?? 0)
      return { ...item, score: finalScore }
    })

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

export type { SearchResult }
// 重新导出 ProviderResult 便于扩展 import
export type { ProviderResult } from './types'
