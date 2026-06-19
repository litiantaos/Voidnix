import { defineExtension } from '@/runtime/extension-registry'
import type { ProviderResult } from '@/runtime/types'
import { copyAndHide } from '@/stores/app'

function pad(n: number): string {
  return n < 10 ? '0' + n : String(n)
}

/** 本地 ISO 风格字符串（带时区偏移，如 2026-06-19T17:40+08:00）。 */
function toLocalIso(date: Date): string {
  const off = -date.getTimezoneOffset()
  const sign = off >= 0 ? '+' : '-'
  const absOff = Math.abs(off)
  return (
    `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}` +
    `T${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}` +
    `${sign}${pad(Math.floor(absOff / 60))}:${pad(absOff % 60)}`
  )
}

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
      if (/^\d{10}$/.test(trimmed)) {
        date = new Date(parseInt(trimmed, 10) * 1000)
        sourceDesc = `Unix 秒 ${trimmed}`
      } else if (/^\d{13}$/.test(trimmed)) {
        date = new Date(parseInt(trimmed, 10))
        sourceDesc = `Unix 毫秒 ${trimmed}`
      } else {
        const d = new Date(trimmed)
        if (!isNaN(d.getTime())) {
          date = d
          sourceDesc = trimmed
        }
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
