import { registerModule } from '@/core/module-registry'
import type { AppModule, SearchResult } from '@/types/module'
import { load } from '@tauri-apps/plugin-store'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { getCurrentWindow } from '@tauri-apps/api/window'

let historyCache: { expr: string; result: string }[] = []
let historyLoaded = false

const loadHistory = async () => {
  if (historyLoaded) return
  try {
    const store = await load('calc_history.json')
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
    const store = await load('calc_history.json')
    await store.set('history', historyCache)
    await store.save()
  } catch (e) {
    console.error('Failed to save calc history:', e)
  }
}

const evaluateMath = (expr: string): string | null => {
  try {
    const withExponent = expr.replace(/\^/g, '**')
    const sanitized = withExponent.replace(/[^0-9+\-*/().%\s*]/g, '')
    if (!sanitized.trim()) return null

    const result = new Function('return ' + sanitized)()
    if (result === undefined || isNaN(result) || !isFinite(result)) return null
    if (Number.isInteger(result)) return result.toString()
    return parseFloat(result.toFixed(6)).toString()
  } catch {
    return null
  }
}

const mod: AppModule = {
  id: 'calculator',
  name: '计算器',
  description: '支持数学表达式计算及历史记录',
  icon: 'i-ri-calculator-line',
  keywords: ['calc', 'calculator', 'math', '计算器', '数学'],
  placeholder: '输入数学表达式',
  order: 2,
  onInit: async () => {
    await loadHistory()
  },
  onSearch: async (query) => {
    if ('calculator'.includes(query.toLowerCase()) || '计算'.includes(query)) {
      return [
        {
          id: 'calc-module',
          title: '计算器',
          description: '打开计算器',
          module: 'calculator',
          icon: 'i-ri-calculator-line',
          score: 100,
          data: { kind: 'module', moduleId: 'calculator' },
        },
      ]
    }

    const withExponent = query.replace(/\^/g, '**')
    const sanitized = withExponent.replace(/[^0-9+\-*/().%\s*]/g, '')
    if (
      sanitized.trim() &&
      sanitized.trim() === query.trim() &&
      /[+\-*/]/.test(sanitized)
    ) {
      try {
        const result = new Function('return ' + sanitized)()
        if (result !== undefined && !isNaN(result) && isFinite(result)) {
          const formatted = Number.isInteger(result) ? result : parseFloat(result.toFixed(6))
          return [
            {
              id: 'calc-quick',
              title: `= ${formatted}`,
              description: `计算: ${query}`,
              module: 'calculator',
              icon: 'i-ri-calculator-line',
              score: 200,
              data: { kind: 'module', expr: query, value: String(formatted) },
            },
          ]
        }
      } catch {
        // ignore
      }
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
      if (
        result.data &&
        !result.data.isHistory &&
        result.data.expr &&
        result.data.value
      ) {
        await saveHistory(
          result.data.expr as string,
          result.data.value as string,
        )
      }
      const value = result.data?.value
        ? String(result.data.value)
        : result.title.replace('= ', '')
      await writeText(value)
      getCurrentWindow().hide()
    } catch (e) {
      console.error('Failed to execute calc item:', e)
    }
  },
}

registerModule(mod)