import type { AppModule, SearchResult } from '@/types/module'
import { keywordSearchAll } from '@/core/module-helpers'

const modules = new Map<string, AppModule>()
const initialized = new Set<string>()

export function registerModule(mod: AppModule) {
  modules.set(mod.id, mod)
}

export function getModule(id: string): AppModule | undefined {
  return modules.get(id)
}

export function getAllModules(): AppModule[] {
  return [...modules.values()]
}

export async function initAllModules() {
  for (const mod of modules.values()) {
    if (mod.onInit && !initialized.has(mod.id)) {
      initialized.add(mod.id)
      try {
        await mod.onInit()
      } catch (e) {
        console.error(`Failed to init module ${mod.id}:`, e)
        initialized.delete(mod.id)
      }
    }
  }
}

const MODULE_SEARCH_TIMEOUT = 3000

// 分组排序上限
const MAX_APP_RESULTS = 30
const MAX_FILE_RESULTS = 50

function withTimeout<T>(promise: Promise<T>, ms: number, moduleId: string): Promise<T> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      console.warn(`[module-registry] Module ${moduleId} search timed out after ${ms}ms`)
      reject(new Error(`Module ${moduleId} timed out`))
    }, ms)
    promise.then(
      (val) => {
        clearTimeout(timer)
        resolve(val)
      },
      (err) => {
        clearTimeout(timer)
        reject(err)
      },
    )
  })
}

/** 分组排序优先级（越小越靠前） */
const GROUP_ORDER: Record<string, number> = {
  application: 0,
  module: 1,
  clipboard: 2,
  'web-search': 3,
  'open-url': 3,
  file: 4,
  folder: 4,
}

function getGroupKey(kind: string | undefined): string {
  if (kind === 'file' || kind === 'folder') return 'file'
  return kind || 'other'
}

/**
 * 分组排序：各组按 GROUP_ORDER 优先级排列，组内按 score 降序。
 */
function groupAndSort(items: SearchResult[]): SearchResult[] {
  const groups = new Map<string, SearchResult[]>()

  for (const item of items) {
    if ((item.score || 0) <= 0) continue
    const key = getGroupKey(item.data?.kind as string | undefined)
    if (!groups.has(key)) groups.set(key, [])
    groups.get(key)!.push(item)
  }

  const sortedGroups = [...groups.entries()].sort(
    (a, b) => (GROUP_ORDER[a[0]] ?? 5) - (GROUP_ORDER[b[0]] ?? 5),
  )

  const result: SearchResult[] = []
  for (const [, groupItems] of sortedGroups) {
    groupItems.sort((a, b) => (b.score || 0) - (a.score || 0))
    const max =
      groupItems[0] &&
      (groupItems[0].data?.kind === 'file' || groupItems[0].data?.kind === 'folder')
        ? MAX_FILE_RESULTS
        : MAX_APP_RESULTS
    result.push(...groupItems.slice(0, max))
  }

  return result
}

export async function searchAll(
  query: string,
  onUpdate?: (results: SearchResult[]) => void,
): Promise<SearchResult[]> {
  const activeModules = getAllModules().filter((m) => m.onSearch)
  const allResults: SearchResult[][] = Array(activeModules.length).fill([])
  let keywordResults: SearchResult[] = []

  const processResults = () => {
    const flattened = [...allResults.flat(), ...keywordResults]
    return groupAndSort(flattened)
  }

  const keywordPromise = query.trim()
    ? keywordSearchAll(query).then((r) => {
        keywordResults = r
        onUpdate?.(processResults())
        return r
      })
    : Promise.resolve([])

  const promises = activeModules.map(async (m, i) => {
    try {
      const res = await withTimeout(m.onSearch!(query), MODULE_SEARCH_TIMEOUT, m.id)
      allResults[i] = res
      onUpdate?.(processResults())
      return res
    } catch (e) {
      console.error(`[module-registry] Module ${m.id} search error:`, e)
      return []
    }
  })

  await Promise.all([keywordPromise.catch(() => []), ...promises])

  return processResults()
}

export async function executeResult(result: SearchResult) {
  const mod = modules.get(result.module)
  if (mod?.onExecute) {
    await mod.onExecute(result)
  }
}
