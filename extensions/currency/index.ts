import { registerModule } from '@/core/module-registry'
import type { AppModule, SearchResult } from '@/types/module'
import { moduleSelfResult } from '@/core/module-helpers'
import { copyAndHide } from '@/utils/clipboard'

const COMMON_CURRENCIES = [
  { code: 'CNY', name: '人民币' },
  { code: 'USD', name: '美元' },
  { code: 'EUR', name: '欧元' },
  { code: 'JPY', name: '日元' },
  { code: 'GBP', name: '英镑' },
  { code: 'HKD', name: '港元' },
  { code: 'KRW', name: '韩元' },
  { code: 'AUD', name: '澳元' },
  { code: 'CAD', name: '加元' },
  { code: 'TRY', name: '土耳其里拉' },
  { code: 'SGD', name: '新加坡元' },
  { code: 'CHF', name: '瑞士法郎' },
  { code: 'NZD', name: '新西兰元' },
  { code: 'RUB', name: '俄罗斯卢布' },
  { code: 'INR', name: '印度卢比' },
]

let ratesCache: Record<string, number> = {}
let isFetching = false

const ensureRates = async () => {
  if (Object.keys(ratesCache).length > 0) return
  if (isFetching) return
  isFetching = true
  try {
    const res = await window.fetch('https://api.exchangerate-api.com/v4/latest/USD', {
      method: 'GET',
    })
    const data = await res.json()
    if (data && data.rates) {
      ratesCache = data.rates
    }
  } catch (e) {
    console.error('Failed to fetch rates:', e)
  } finally {
    isFetching = false
  }
}

const mod: AppModule = {
  id: 'currency',
  name: '汇率转换',
  description: '实时汇率转换计算',
  icon: 'i-ri-money-cny-circle-line',
  keywords: ['currency', 'exchange', 'rate', '汇率', '货币', '钱'],
  placeholder: '默认基于 1 CNY 计算，也可输入 100 USD 或 100 USD TO CNY',
  order: 3,
  onInit: async () => {
    ensureRates()
  },
  onSearch: async (query) => {
    if (!query.trim()) return []
    if ('currency'.includes(query.toLowerCase()) || '汇率'.includes(query)) {
      return [moduleSelfResult(mod)]
    }
    return []
  },
  onModuleSearch: async (query) => {
    await ensureRates()
    if (Object.keys(ratesCache).length === 0) {
      return [
        {
          id: 'err',
          title: '无法获取汇率',
          description: '请检查网络连接',
          module: 'currency',
          icon: 'i-ri-error-warning-line',
        },
      ]
    }

    let amount = 1
    let baseCurrency = 'USD'
    let targetCurrency = ''

    const trimmed = query.trim().toUpperCase()
    const match = trimmed.match(/^([\d.]+)?\s*([A-Za-z$€£¥]+)?(?:\s+(?:TO|->|>)\s+([A-Za-z]+))?$/i)

    if (match) {
      if (match[1]) amount = parseFloat(match[1])

      let maybeBase = match[2]
      if (maybeBase === '¥' || maybeBase === 'RMB') maybeBase = 'CNY'
      if (maybeBase === '$') maybeBase = 'USD'
      if (maybeBase === '€') maybeBase = 'EUR'
      if (maybeBase === '£') maybeBase = 'GBP'

      if (maybeBase && ratesCache[maybeBase]) {
        baseCurrency = maybeBase
      } else {
        baseCurrency = 'CNY'
        if (!match[1]) {
          amount = 1
        }
      }

      if (match[3] && ratesCache[match[3]]) {
        targetCurrency = match[3]
      }
    } else if (trimmed === '') {
      baseCurrency = 'CNY'
      amount = 1
    } else {
      baseCurrency = 'CNY'
      amount = parseFloat(trimmed) || 1
    }

    const results: SearchResult[] = []
    const baseRate = ratesCache[baseCurrency] || 1

    const targetList = targetCurrency
      ? COMMON_CURRENCIES.filter((c) => c.code === targetCurrency)
      : COMMON_CURRENCIES.filter((c) => c.code !== baseCurrency)

    for (const target of targetList) {
      const targetRate = ratesCache[target.code]
      if (!targetRate) continue

      const converted = (amount / baseRate) * targetRate
      const formatted =
        converted >= 0.01
          ? converted.toFixed(2)
          : converted.toPrecision(4).replace(/0+$/, '').replace(/\.$/, '')

      results.push({
        id: target.code,
        title: `${formatted} ${target.code}`,
        description: target.name,
        module: 'currency',
        icon: 'i-ri-money-cny-circle-line',
        data: { isHighlight: true },
      })
    }

    return results
  },
  onExecute: async (result) => {
    if (result.id === 'err') return
    try {
      await copyAndHide(result.title.split(' ')[0])
    } catch (e) {
      console.error('Failed to copy currency:', e)
    }
  },
}

registerModule(mod)
