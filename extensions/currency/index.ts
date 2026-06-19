import { defineExtension } from '@/runtime/extension-registry'
import type { ProviderResult } from '@/runtime/types'
import { copyAndHide } from '@/utils/clipboard'
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
    dynamic: async (query): Promise<ProviderResult[]> => {
      const parsed = parseCurrencyInput(query)
      if (!parsed) return []

      const { amount, fromCurrency } = parsed

      const rates = await fetchRates()
      if (!rates || !rates[fromCurrency]) return []

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
