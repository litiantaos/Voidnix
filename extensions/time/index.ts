import { defineExtension } from '@/runtime/extension-registry'
import type { ProviderResult } from '@/runtime/types'
import { copyAndHide } from '@/stores/app'
import { toLocalIso, parseTimestamp } from './logic'

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

      const mk = (id: string, title: string, desc: string): ProviderResult => ({
        id,
        title,
        description: desc,
        icon: 'i-ri-time-line',
        data: { kind: 'module', value: title },
      })

      if (!trimmed) {
        const now = Date.now()
        const sec = Math.floor(now / 1000)
        const d = new Date(now)
        results.push(mk('time-now-sec', String(sec), 'Unix 时间戳（秒）'))
        results.push(mk('time-now-ms', String(now), 'Unix 时间戳（毫秒）'))
        results.push(mk('time-now-iso', toLocalIso(d), '本地时间（ISO）'))
        results.push(mk('time-now-utc', d.toUTCString(), 'UTC 时间'))
        return results
      }

      let date: Date | null = null
      let sourceDesc = ''
      const parsed = parseTimestamp(trimmed)
      if (parsed) {
        date = new Date(parsed.ts)
        sourceDesc = /^\d{10}$/.test(trimmed)
          ? `Unix 秒 ${trimmed}`
          : /^\d{13}$/.test(trimmed)
            ? `Unix 毫秒 ${trimmed}`
            : trimmed
      }

      if (date) {
        const ms = date.getTime()
        results.push(mk('ts-date', date.toLocaleString('zh-CN'), sourceDesc))
        results.push(mk('ts-sec', String(Math.floor(ms / 1000)), '→ Unix 时间戳（秒）'))
        results.push(mk('ts-ms', String(ms), '→ Unix 时间戳（毫秒）'))
        results.push(mk('ts-iso', toLocalIso(date), '→ 本地 ISO'))
        results.push(mk('ts-utc', date.toUTCString(), '→ UTC'))
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
