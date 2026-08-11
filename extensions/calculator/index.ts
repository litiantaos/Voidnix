import { defineExtension } from '@/runtime/extension-registry'
import type { ProviderResult } from '@/runtime/types'
import { copyAndHide } from '@/stores/app'
import { evaluateMath } from './logic'
import { config, appendHistory } from './config'

export default defineExtension({
  meta: {
    id: 'calculator',
    name: { 'zh-CN': '计算器', en: 'Calculator' },
    description: { 'zh-CN': '数学表达式计算', en: 'Math expression calculator' },
    icon: 'i-ri-calculator-line',
    keywords: ['calc', 'calculator', 'math', '计算器', '数学'],
    order: 90,
  },

  placeholder: { 'zh-CN': '输入数学表达式', en: 'Enter a math expression' },

  search: {
    dynamic: (query, ctx): ProviderResult[] => {
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
              kind: 'extension',
              isHighlight: true,
              isHistory: false,
              expr: trimmed,
              value: res,
            },
          })
        }
      }

      // 全局模式只返回即时计算结果；history 仅扩展内展示（避免不相关项误杀扩展入口）
      if (ctx?.extensionMode) {
        config.history.forEach((h, idx) => {
          results.push({
            id: `history-${idx}`,
            title: `= ${h.result}`,
            description: h.expr,
            icon: 'i-ri-history-line',
            data: { kind: 'extension', isHistory: true, expr: h.expr, value: h.result },
          })
        })
      }

      return results
    },
  },

  onExecute: async (result) => {
    try {
      if (result.data && !result.data.isHistory && result.data.expr && result.data.value) {
        appendHistory(result.data.expr as string, result.data.value as string)
      }
      const value = result.data?.value ? String(result.data.value) : result.title.replace('= ', '')
      await copyAndHide(value)
    } catch (e) {
      console.error('Failed to execute calc item:', e)
    }
  },
})
