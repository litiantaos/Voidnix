import { defineExtension } from '@/runtime/extension-registry'
import type { ProviderResult } from '@/runtime/types'
import { load } from '@tauri-apps/plugin-store'
import { writeText } from '@/utils/clipboard'
import { hideWindow } from '@/utils/tauri'
import { evaluateMath } from './logic'

let historyCache: { expr: string; result: string }[] = []
let historyLoaded = false

async function loadHistory() {
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

async function saveHistory(expr: string, result: string) {
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

export default defineExtension({
  meta: {
    id: 'calculator',
    name: '计算器',
    description: '数学表达式计算',
    icon: 'i-ri-calculator-line',
    keywords: ['calc', 'calculator', 'math', '计算器', '数学'],
    order: 2,
  },

  placeholder: '输入数学表达式',
  hints: { enter: '复制' },

  setup: async () => {
    await loadHistory()
  },

  search: {
    dynamic: async (query): Promise<ProviderResult[]> => {
      await loadHistory()
      const results: ProviderResult[] = []
      const trimmed = query.trim()

      if (trimmed) {
        const res = evaluateMath(trimmed)
        if (res !== null) {
          results.push({
            id: 'current',
            title: `= ${res}`,
            description: trimmed,
            icon: 'i-ri-calculator-line',
            data: {
              kind: 'module',
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
          icon: 'i-ri-history-line',
          data: { kind: 'module', isHistory: true, expr: h.expr, value: h.result },
        })
      })

      return results
    },
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
})
