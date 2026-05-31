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

export async function searchAll(
  query: string,
  onUpdate?: (results: SearchResult[]) => void,
): Promise<SearchResult[]> {
  const activeModules = getAllModules().filter((m) => m.onSearch)
  const allResults: SearchResult[][] = Array(activeModules.length).fill([])
  let keywordResults: SearchResult[] = []

  const processResults = () => {
    const flattened = [...allResults.flat(), ...keywordResults]
    const filtered = flattened.filter((item) => (item.score || 0) > 0)
    filtered.sort((a, b) => (b.score || 0) - (a.score || 0))
    return filtered.slice(0, 80)
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
