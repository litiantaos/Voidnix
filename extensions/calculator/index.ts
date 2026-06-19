import { registerModule } from '@/core/module-registry'
import type { AppModule, SearchResult } from '@/types/module'
import { load } from '@tauri-apps/plugin-store'
import { writeText } from '@/utils/clipboard'
import { hideWindow } from '@/utils/tauri'
import { evaluateMath } from './logic'

let historyCache: { expr: string; result: string }[] = []
let historyLoaded = false

const loadHistory = async () => {
  if (historyLoaded) return
  try {
    const store = await load('extensions/calculator/calc_history.json')
    const saved = await store.get<{ expr: string; result: string }[]>('history')
    if (saved && Array.isArray(saved)) {
      historyCache = saved
    }
    historyLoaded = true
  } catch (e) {
    console.error('Failed to load calc history:', e)
  }
}

const saveHistory = async (expr: string, result: string) => {
  try {
    if (historyCache.length > 0 && historyCache[0].expr === expr) {
      return
    }
    historyCache.unshift({ expr, result })
    if (historyCache.length > 10) {
      historyCache = historyCache.slice(0, 10)
    }
    const store = await load('extensions/calculator/calc_history.json')
    await store.set('history', historyCache)
    await store.save()
  } catch (e) {
    console.error('Failed to save calc history:', e)
  }
}

const mod: AppModule = {
  id: 'calculator',
  name: '计算器',
  description: '数学表达式计算',
  icon: 'i-ri-calculator-line',
  keywords: ['calc', 'calculator', 'math', '计算器', '数学'],
  placeholder: '输入数学表达式',
  order: 2,
  enterHint: '复制',
  onInit: async () => {
    await loadHistory()
  },
  onSearch: async (query) => {
    if (!query.trim()) return []

    const withExponent = query.replace(/\^/g, '**')
    if (
      withExponent.trim() &&
      /^[0-9+\-*/().%\s]*$/.test(withExponent) &&
      /[+\-*/]/.test(withExponent)
    ) {
      try {
        const result = evaluateMath(query)
        if (result !== null) {
          return [
            {
              id: 'calc-quick',
              title: `= ${result}`,
              description: `计算: ${query}`,
              module: 'calculator',
              icon: 'i-ri-calculator-line',
              score: 2000,
              data: { kind: 'module', expr: query, value: result },
            },
          ]
        }
      } catch {}
    }
    return []
  },
  onModuleSearch: async (query) => {
    await loadHistory()
    const results: SearchResult[] = []
    const trimmed = query.trim()

    if (trimmed) {
      const res = evaluateMath(trimmed)
      if (res !== null) {
        results.push({
          id: 'current',
          title: `= ${res}`,
          description: trimmed,
          module: 'calculator',
          icon: 'i-ri-calculator-line',
          data: {
            isHighlight: true,
            isHistory: false,
            expr: trimmed,
            value: res,
          },
        })
      }
    }

    historyCache.forEach((h, idx) => {
      results.push({
        id: `history-${idx}`,
        title: `= ${h.result}`,
        description: h.expr,
        module: 'calculator',
        icon: 'i-ri-history-line',
        data: { isHistory: true, expr: h.expr, value: h.result },
      })
    })

    return results
  },
  onExecute: async (result) => {
    try {
      if (result.data && !result.data.isHistory && result.data.expr && result.data.value) {
        await saveHistory(result.data.expr as string, result.data.value as string)
      }
      const value = result.data?.value ? String(result.data.value) : result.title.replace('= ', '')
      await writeText(value)
      hideWindow()
    } catch (e) {
      console.error('Failed to execute calc item:', e)
    }
  },
}

registerModule(mod)
