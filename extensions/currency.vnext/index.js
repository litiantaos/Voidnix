var ratesCache = null
var isFetching = false

var COMMON_CURRENCIES = [
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

async function ensureRates(ctx) {
  if (ratesCache && Object.keys(ratesCache).length > 0) return true
  if (isFetching) return false
  isFetching = true
  try {
    var result = await ctx.http.fetch('https://api.exchangerate-api.com/v4/latest/USD')
    var data = JSON.parse(result.body)
    if (data && data.rates) {
      ratesCache = data.rates
      return true
    }
  } catch {
  } finally {
    isFetching = false
  }
  return false
}

export default {
  id: 'currency',

  async onInit(_ctx) {
    // rates will be fetched on first search
  },

  async onSearch(query, ctx) {
    if (!(await ensureRates(ctx))) {
      return {
        type: 'list',
        items: [
          {
            id: 'err',
            title: '无法获取汇率',
            subtitle: '请检查网络连接',
            icon: 'i-ri-error-warning-line',
          },
        ],
      }
    }

    var amount = 1
    var baseCurrency = 'USD'
    var targetCurrency = ''

    var trimmed = (query || '').trim().toUpperCase()
    var match = trimmed.match(/^([\d.]+)?\s*([A-Za-z$€£¥]+)?(?:\s+(?:TO|->|>)\s+([A-Za-z]+))?$/i)

    if (match) {
      if (match[1]) amount = parseFloat(match[1])

      var maybeBase = match[2]
      if (maybeBase === '¥' || maybeBase === 'RMB') maybeBase = 'CNY'
      if (maybeBase === '$') maybeBase = 'USD'
      if (maybeBase === '€') maybeBase = 'EUR'
      if (maybeBase === '£') maybeBase = 'GBP'

      if (maybeBase && ratesCache[maybeBase]) {
        baseCurrency = maybeBase
      } else {
        baseCurrency = 'CNY'
        if (!match[1]) amount = 1
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

    var items = []
    var baseRate = ratesCache[baseCurrency] || 1

    var targetList = targetCurrency
      ? COMMON_CURRENCIES.filter(function (c) {
          return c.code === targetCurrency
        })
      : COMMON_CURRENCIES.filter(function (c) {
          return c.code !== baseCurrency
        })

    for (var i = 0; i < targetList.length; i++) {
      var target = targetList[i]
      var targetRate = ratesCache[target.code]
      if (!targetRate) continue

      var converted = (amount / baseRate) * targetRate
      var formatted =
        converted >= 0.01
          ? converted.toFixed(2)
          : converted.toPrecision(4).replace(/0+$/, '').replace(/\.$/, '')

      items.push({
        id: target.code,
        title: formatted,
        subtitle: target.code + ' ' + target.name,
      })
    }

    return { type: 'list', items: items }
  },
}
