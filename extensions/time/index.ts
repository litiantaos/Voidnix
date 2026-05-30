import { registerModule } from '@/core/module-registry'
import type { AppModule, SearchResult } from '@/types/module'
import { moduleSelfResult } from '@/core/module-helpers'
import { copyAndHide } from '@/utils/clipboard'

const mod: AppModule = {
  id: 'time',
  name: '时间戳',
  description: '时间与时间戳转换',
  icon: 'i-ri-time-line',
  keywords: ['time', 'date', 'timestamp', '时间', '时间戳', '日期'],
  placeholder: '输入时间戳或日期字符串',
  order: 4,
  onSearch: async (query) => {
    if (!query.trim()) return []
    if (
      'time'.includes(query.toLowerCase()) ||
      'date'.includes(query.toLowerCase()) ||
      'timestamp'.includes(query.toLowerCase()) ||
      '时间'.includes(query)
    ) {
      return [moduleSelfResult(mod)]
    }
    return []
  },
  onModuleSearch: async (query) => {
    const trimmed = query.trim()
    const results: SearchResult[] = []

    if (!trimmed) {
      const now = new Date()
      results.push(
        {
          id: 'local',
          title: now.toLocaleString('zh-CN', { hour12: false }),
          description: '当前时间 (Local)',
          module: 'time',
          icon: 'i-ri-time-line',
        },
        {
          id: 'ms',
          title: now.getTime().toString(),
          description: '时间戳 (毫秒)',
          module: 'time',
          icon: 'i-ri-timer-line',
        },
        {
          id: 's',
          title: Math.floor(now.getTime() / 1000).toString(),
          description: '时间戳 (秒)',
          module: 'time',
          icon: 'i-ri-timer-line',
        },
      )
      return results
    }

    if (/^\d+$/.test(trimmed)) {
      const num = parseInt(trimmed, 10)
      const dateMs = num > 9999999999 ? new Date(num) : new Date(num * 1000)
      if (!isNaN(dateMs.getTime())) {
        results.push({
          id: 'parsed-time',
          title: dateMs.toLocaleString('zh-CN', { hour12: false }),
          description: `由时间戳 ${trimmed} 解析`,
          module: 'time',
          icon: 'i-ri-calendar-line',
        })
      }
    }

    const parsedDate = new Date(trimmed)
    if (!isNaN(parsedDate.getTime())) {
      results.push(
        {
          id: 'parsed-ms',
          title: parsedDate.getTime().toString(),
          description: `由日期解析 (毫秒)`,
          module: 'time',
          icon: 'i-ri-timer-line',
        },
        {
          id: 'parsed-s',
          title: Math.floor(parsedDate.getTime() / 1000).toString(),
          description: `由日期解析 (秒)`,
          module: 'time',
          icon: 'i-ri-timer-line',
        },
      )
    }

    return results
  },
  onExecute: async (result) => {
    try {
      await copyAndHide(result.title)
    } catch (e) {
      console.error('Failed to copy time:', e)
    }
  },
}

registerModule(mod)