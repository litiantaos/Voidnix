import { defineExtension } from '@/runtime/extension-registry'
import type { ProviderResult } from '@/runtime/types'
import { copyAndHide } from '@/stores/app'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import {
  CURRENCIES,
  parseCurrencyInput,
  convertCurrency,
  isRatesCacheFresh,
  formatWithChineseUnit,
  CURRENCY_CODE_TO_NAME,
} from './logic'

/** 换算结果 title 为纯数值、与 query 无 fuzzy 命中；
 *  全局 groupAndSort 零分过滤会丢弃，boost 保证其出现并优先于模块入口（KEYWORD_MODULE_BOOST=500）。 */
const DYNAMIC_BOOST = 1000

let ratesCache: Record<string, number> | null = null
let ratesTime = 0

async function fetchRates(): Promise<Record<string, number> | null> {
  const now = Date.now()
  if (ratesCache && isRatesCacheFresh(ratesTime, now)) return ratesCache

  try {
    // 走框架 Rust http_get：绕过 webview UA/Referer 反爬与 CORS（与 ip 扩展一致）
    const text = await invoke<string>(CMD.httpGet, { url: 'https://open.er-api.com/v6/latest/USD' })
    const data = JSON.parse(text) as { rates?: Record<string, number> }
    if (data?.rates) {
      ratesCache = data.rates
      ratesTime = now
    }
  } catch {
    // 网络错误时用缓存（如有）
  }
  return ratesCache
}

export default defineExtension({
  meta: {
    id: 'currency',
    name: '汇率',
    description: '货币汇率换算',
    icon: 'i-ri-exchange-cny-line',
    order: 100,
    keywords: ['汇率', '货币', 'currency', 'exchange', 'usd', 'cny', 'eur', 'jpy'],
  },

  placeholder: '输入金额和货币代码，如 100 USD、1万美元、3亿日元，默认查询 1 USD',
  hints: { enter: '复制' },

  search: {
    dynamic: async (query, ctx): Promise<ProviderResult[]> => {
      const trimmed = query.trim()
      // 全局默认列表（moduleMode=false）空 query 不触发网络请求（避免拖慢主列表）；
      // 模块内空 query 展示参考汇率
      if (!trimmed && !ctx?.moduleMode) return []

      const rates = await fetchRates()
      if (!rates) return []

      // 空 query：展示以 USD 为基准的参考汇率，避免进模块见空
      if (!trimmed) {
        const base = 'USD'
        if (!rates[base]) return []
        return CURRENCIES.filter((c) => c !== base).map((to) => {
          const v = convertCurrency(1, base, to, rates)
          const cnName = CURRENCY_CODE_TO_NAME[to]
          return {
            id: `ref-${base}-${to}`,
            title: v.toFixed(4),
            description: `${cnName ? cnName + ' ' : ''}${to}`,
            icon: 'i-ri-exchange-cny-line',
            data: { kind: 'module', value: v.toFixed(4) },
          }
        })
      }

      const parsed = parseCurrencyInput(query)
      if (!parsed) return []

      const { amount, fromCurrency } = parsed
      if (!rates[fromCurrency]) return []

      const results: ProviderResult[] = []

      // 全局模式只返回主换算（第一个非源货币，通常 CNY），避免多条刷屏；
      // 模块模式返回全部目标货币
      const primary = CURRENCIES.find((c) => c !== fromCurrency) ?? 'CNY'
      const targets = ctx?.moduleMode ? CURRENCIES.filter((c) => c !== fromCurrency) : [primary]

      for (const toCurrency of targets) {
        const converted = convertCurrency(amount, fromCurrency, toCurrency, rates)
        const cnName = CURRENCY_CODE_TO_NAME[toCurrency]
        // 仅在结果被量词格式化时（≥1万）附加，未格式化时与 title 重复不显示
        const suffix = Math.abs(converted) >= 1e4 ? ` · ${formatWithChineseUnit(converted)}` : ''
        results.push({
          id: `currency-${toCurrency}`,
          title: converted.toFixed(2),
          description: `${cnName ? cnName + ' ' : ''}${toCurrency}${suffix}`,
          icon: 'i-ri-exchange-cny-line',
          boost: DYNAMIC_BOOST,
          data: { kind: 'module', value: converted.toFixed(2) },
        })
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
