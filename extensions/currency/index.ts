import { defineExtension } from '@/runtime/extension-registry'
import type { ProviderResult } from '@/runtime/types'
import { copyAndHide } from '@/stores/app'
import { CURRENCIES, parseCurrencyInput, convertCurrency, isRatesCacheFresh } from './logic'

let ratesCache: Record<string, number> | null = null
let ratesTime = 0

async function fetchRates(): Promise<Record<string, number> | null> {
  const now = Date.now()
  if (ratesCache && isRatesCacheFresh(ratesTime, now)) return ratesCache

  try {
    const res = await fetch('https://open.er-api.com/v6/latest/USD')
    if (!res.ok) return ratesCache
    const data = await res.json()
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
    order: 130,
    keywords: ['汇率', '货币', 'currency', 'exchange', 'usd', 'cny', 'eur', 'jpy'],
  },

  placeholder: '输入金额和货币代码，如 100 USD 或 100 美元',

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
          return {
            id: `ref-${base}-${to}`,
            title: `${v.toFixed(4)} ${to}`,
            description: `1 ${base} → ${to}（参考汇率）`,
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

      for (const toCurrency of CURRENCIES) {
        if (toCurrency === fromCurrency) continue
        const converted = convertCurrency(amount, fromCurrency, toCurrency, rates)
        results.push({
          id: `currency-${toCurrency}`,
          title: `${converted.toFixed(2)} ${toCurrency}`,
          description: `${amount} ${fromCurrency} → ${toCurrency}`,
          icon: 'i-ri-exchange-cny-line',
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
