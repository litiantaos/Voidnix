import { defineExtension } from '@/runtime/extension-registry'
import type { ProviderResult } from '@/runtime/types'
import { writeText } from '@/utils/clipboard'
import { hideWindow } from '@/utils/tauri'
import { evaluateMath } from './logic'
import { config, appendHistory } from './config'

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

  search: {
    dynamic: async (query): Promise<ProviderResult[]> => {
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

      config.history.forEach((h, idx) => {
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
        appendHistory(result.data.expr as string, result.data.value as string)
      }
      const value = result.data?.value ? String(result.data.value) : result.title.replace('= ', '')
      await writeText(value)
      hideWindow()
    } catch (e) {
      console.error('Failed to execute calc item:', e)
    }
  },
})
