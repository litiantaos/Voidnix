import { registerModule } from '@/core/module-registry'
import type { AppModule, SearchResult } from '@/types/module'
import { copyAndHide } from '@/utils/clipboard'

const module: AppModule = {
  id: 'time',
  name: '时间戳',
  description: 'Unix 时间戳 ↔ 日期转换',
  icon: 'i-ri-time-line',
  order: 120,
  keywords: ['时间', '时间戳', 'timestamp', 'date', 'unix', 'epoch'],

  placeholder: '输入 Unix 时间戳或日期进行转换',

  onModuleSearch: async (query: string): Promise<SearchResult[]> => {
    const trimmed = query.trim()
    const results: SearchResult[] = []

    if (!trimmed) {
      const now = Math.floor(Date.now() / 1000)
      results.push({
        id: 'time-now',
        title: String(now),
        module: 'time',
        description: new Date().toLocaleString('zh-CN'),
        icon: 'i-ri-time-line',
        score: 1000,
        data: { kind: 'result', value: String(now) },
      })
      return results
    }

    // 数字 → 当作时间戳解析
    if (/^\d{10}$/.test(trimmed)) {
      const ts = parseInt(trimmed, 10)
      const date = new Date(ts * 1000)
      results.push({
        id: 'time-ts-to-date',
        title: date.toLocaleString('zh-CN'),
        module: 'time',
        description: `Unix 时间戳 ${trimmed}（秒）`,
        icon: 'i-ri-time-line',
        score: 1000,
        data: { kind: 'result', value: date.toLocaleString('zh-CN') },
      })
    } else if (/^\d{13}$/.test(trimmed)) {
      const ts = parseInt(trimmed, 10)
      const date = new Date(ts)
      results.push({
        id: 'time-ms-to-date',
        title: date.toLocaleString('zh-CN'),
        module: 'time',
        description: `毫秒时间戳 ${trimmed}`,
        icon: 'i-ri-time-line',
        score: 1000,
        data: { kind: 'result', value: date.toLocaleString('zh-CN') },
      })
    } else {
      // 尝试解析为日期 → 时间戳
      const date = new Date(trimmed)
      if (!isNaN(date.getTime())) {
        results.push({
          id: 'time-date-to-ts',
          title: String(Math.floor(date.getTime() / 1000)),
          module: 'time',
          description: `${date.toLocaleString('zh-CN')} → Unix 时间戳（秒）`,
          icon: 'i-ri-time-line',
          score: 1000,
          data: { kind: 'result', value: String(Math.floor(date.getTime() / 1000)) },
        })
      }
    }

    return results
  },

  async onExecute(result: SearchResult) {
    if (result.data?.value) {
      copyAndHide(result.data.value as string)
    }
  },
}

registerModule(module)
