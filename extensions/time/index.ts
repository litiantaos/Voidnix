import { defineExtension } from '@/runtime/extension-registry'
import type { ProviderResult } from '@/runtime/types'
import { copyAndHide } from '@/stores/app'
import { t } from '@/runtime/i18n'
import { toLocalIso, parseTimestamp } from './logic'

import './locales'

export default defineExtension({
  meta: {
    id: 'time',
    name: { 'zh-CN': '时间戳', en: 'Timestamp' },
    description: { 'zh-CN': 'Unix 时间戳与转换', en: 'Unix timestamp conversion' },
    icon: 'i-ri-time-line',
    order: 50,
    keywords: ['时间', '时间戳', 'timestamp', 'date', 'unix', 'epoch'],
  },

  placeholder: {
    'zh-CN': '输入 Unix 时间戳或日期进行转换',
    en: 'Enter Unix timestamp or date to convert',
  },

  search: {
    dynamic: (query, ctx): ProviderResult[] => {
      // 仅扩展内转换（全局避免日期/时间戳形态误触）
      if (!ctx?.extensionMode) return []
      const trimmed = query.trim()
      const results: ProviderResult[] = []

      const mk = (id: string, title: string, desc: string): ProviderResult => ({
        id,
        title,
        description: desc,
        icon: 'i-ri-time-line',
        data: { kind: 'extension', value: title },
      })

      if (!trimmed) {
        const now = Date.now()
        const sec = Math.floor(now / 1000)
        const d = new Date(now)
        results.push(mk('time-now-sec', String(sec), t('time.unixSec')))
        results.push(mk('time-now-ms', String(now), t('time.unixMs')))
        results.push(mk('time-now-iso', toLocalIso(d), t('time.localIso')))
        results.push(mk('time-now-utc', d.toUTCString(), t('time.utc')))
        return results
      }

      let date: Date | null = null
      let sourceDesc = ''
      const parsed = parseTimestamp(trimmed)
      if (parsed) {
        date = new Date(parsed.ts)
        sourceDesc = /^\d{10}$/.test(trimmed)
          ? t('time.sourceSec', { value: trimmed })
          : /^\d{13}$/.test(trimmed)
            ? t('time.sourceMs', { value: trimmed })
            : trimmed
      }

      if (date) {
        const ms = date.getTime()
        results.push(mk('ts-date', date.toLocaleString(), sourceDesc))
        results.push(mk('ts-sec', String(Math.floor(ms / 1000)), t('time.toUnixSec')))
        results.push(mk('ts-ms', String(ms), t('time.toUnixMs')))
        results.push(mk('ts-iso', toLocalIso(date), t('time.toLocalIso')))
        results.push(mk('ts-utc', date.toUTCString(), t('time.toUtc')))
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
