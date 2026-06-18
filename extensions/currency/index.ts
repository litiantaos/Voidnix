import { registerModule } from '@/core/module-registry'
import type { AppModule, SearchResult } from '@/types/module'
import { copyAndHide } from '@/utils/clipboard'

let ratesCache: Record<string, number> | null = null
let ratesTime = 0

async function fetchRates(): Promise<Record<string, number> | null> {
  const now = Date.now()
  if (ratesCache && now - ratesTime < 10 * 60 * 1000) return ratesCache

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

const CURRENCIES = ['CNY', 'USD', 'EUR', 'JPY', 'GBP', 'HKD', 'KRW', 'TWD']

const module: AppModule = {
  id: 'currency',
  name: '汇率',
  description: '货币汇率换算',
  icon: 'i-ri-exchange-cny-line',
  order: 130,
  keywords: ['汇率', '货币', 'currency', 'exchange', 'usd', 'cny', 'eur', 'jpy'],

  placeholder: '输入金额和货币代码，如 100 USD 或 100 美元',

  onModuleSearch: async (query: string): Promise<SearchResult[]> => {
    const trimmed = query.trim()
    if (!trimmed) return []

    // 解析 "100 USD" / "100usd" / "100美元" 格式
    const match = trimmed.match(
      /^(\d+(?:\.\d+)?)\s*([A-Za-z]{3}|美元|人民币|欧元|日元|英镑|港币|韩元|台币)$/,
    )
    if (!match) return []

    const amount = parseFloat(match[1])
    const currencyMap: Record<string, string> = {
      美元: 'USD',
      人民币: 'CNY',
      欧元: 'EUR',
      日元: 'JPY',
      英镑: 'GBP',
      港币: 'HKD',
      韩元: 'KRW',
      台币: 'TWD',
    }
    const fromCurrency = currencyMap[match[2]] || match[2].toUpperCase()

    const rates = await fetchRates()
    if (!rates || !rates[fromCurrency]) return []

    const usdAmount = amount / rates[fromCurrency]
    const results: SearchResult[] = []

    for (const toCurrency of CURRENCIES) {
      if (toCurrency === fromCurrency) continue
      const converted = usdAmount * rates[toCurrency]
      results.push({
        id: `currency-${toCurrency}`,
        title: `${converted.toFixed(2)} ${toCurrency}`,
        module: 'currency',
        description: `${amount} ${fromCurrency} → ${toCurrency}`,
        icon: 'i-ri-exchange-cny-line',
        score: 1000 - CURRENCIES.indexOf(toCurrency),
        data: { kind: 'result', value: converted.toFixed(2) },
      })
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
