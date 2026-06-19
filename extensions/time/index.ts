import { defineExtension } from '@/runtime/extension-registry'
import type { ProviderResult } from '@/runtime/types'
import { copyAndHide } from '@/utils/clipboard'

export default defineExtension({
  meta: {
    id: 'time',
    name: '时间戳',
    description: 'Unix 时间戳 ↔ 日期转换',
    icon: 'i-ri-time-line',
    order: 120,
    keywords: ['时间', '时间戳', 'timestamp', 'date', 'unix', 'epoch'],
  },

  placeholder: '输入 Unix 时间戳或日期进行转换',

  search: {
    dynamic: (query): ProviderResult[] => {
      const trimmed = query.trim()
      const results: ProviderResult[] = []

      if (!trimmed) {
        const now = Math.floor(Date.now() / 1000)
        results.push({
          id: 'time-now',
          title: String(now),
          description: new Date().toLocaleString('zh-CN'),
          icon: 'i-ri-time-line',
          data: { kind: 'module', value: String(now) },
        })
        return results
      }

      if (/^\d{10}$/.test(trimmed)) {
        const ts = parseInt(trimmed, 10)
        const date = new Date(ts * 1000)
        results.push({
          id: 'time-ts-to-date',
          title: date.toLocaleString('zh-CN'),
          description: `Unix 时间戳 ${trimmed}（秒）`,
          icon: 'i-ri-time-line',
          data: { kind: 'module', value: date.toLocaleString('zh-CN') },
        })
      } else if (/^\d{13}$/.test(trimmed)) {
        const ts = parseInt(trimmed, 10)
        const date = new Date(ts)
        results.push({
          id: 'time-ms-to-date',
          title: date.toLocaleString('zh-CN'),
          description: `毫秒时间戳 ${trimmed}`,
          icon: 'i-ri-time-line',
          data: { kind: 'module', value: date.toLocaleString('zh-CN') },
        })
      } else {
        const date = new Date(trimmed)
        if (!isNaN(date.getTime())) {
          results.push({
            id: 'time-date-to-ts',
            title: String(Math.floor(date.getTime() / 1000)),
            description: `${date.toLocaleString('zh-CN')} → Unix 时间戳（秒）`,
            icon: 'i-ri-time-line',
            data: { kind: 'module', value: String(Math.floor(date.getTime() / 1000)) },
          })
        }
      }

      return results
    },
  },

  onExecute: async (result) => {
    if (result.data?.value) {
      copyAndHide(result.data.value as string)
    }
  },
})
