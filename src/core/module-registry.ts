import type { AppModule, SearchResult } from '@/types/module'

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

export async function activateModule(id: string) {
  const mod = modules.get(id)
  if (mod?.onActivate) {
    await mod.onActivate()
  }
}

export async function deactivateModule(id: string) {
  const mod = modules.get(id)
  if (mod?.onDeactivate) {
    await mod.onDeactivate()
  }
}

const MODULE_SEARCH_TIMEOUT = 3000
const OVERALL_SEARCH_TIMEOUT = 8000

function withTimeout<T>(
  promise: Promise<T>,
  ms: number,
  moduleId: string,
): Promise<T> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      console.warn(
        `[module-registry] Module ${moduleId} search timed out after ${ms}ms`,
      )
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

  const processResults = () => {
    const flattened = allResults.flat()

    const groups = new Map<
      string,
      { maxScore: number; items: SearchResult[] }
    >()
    for (const item of flattened) {
      let kind = item.data?.kind || 'other'

      if (kind === 'file' || kind === 'folder') {
        kind = 'file_and_folder'
      }
      const score = item.score || 0
      if (!groups.has(kind)) {
        groups.set(kind, { maxScore: score, items: [] })
      }
      const group = groups.get(kind)!
      group.items.push(item)
      if (score > group.maxScore) {
        group.maxScore = score
      }
    }

    const sortedGroups = Array.from(groups.values()).sort(
      (a, b) => b.maxScore - a.maxScore,
    )

    const finalResults: SearchResult[] = []
    for (const group of sortedGroups) {
      group.items.sort((a, b) => (b.score || 0) - (a.score || 0))
      finalResults.push(...group.items)
    }

    return finalResults.slice(0, 80)
  }

  const promises = activeModules.map(async (m, i) => {
    try {
      const res = await withTimeout(
        m.onSearch!(query),
        MODULE_SEARCH_TIMEOUT,
        m.id,
      )
      allResults[i] = res
      if (onUpdate) {
        onUpdate(processResults())
      }
      return res
    } catch (e) {
      console.error(`[module-registry] Module ${m.id} search error:`, e)
      return []
    }
  })

  const overallPromise = Promise.all(promises)
  try {
    await withTimeout(overallPromise, OVERALL_SEARCH_TIMEOUT, 'overall')
  } catch {
    console.warn(
      '[module-registry] Overall search timed out, returning partial results',
    )
  }

  return processResults()
}

export async function executeResult(result: SearchResult) {
  const mod = modules.get(result.module)
  if (mod?.onExecute) {
    await mod.onExecute(result)
  }
}
