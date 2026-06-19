export const CURRENCIES = ['CNY', 'USD', 'EUR', 'JPY', 'GBP', 'HKD', 'KRW', 'TWD']

/** 中文名 → ISO 代码映射；未命中的代码原样大写。 */
export const CURRENCY_NAME_MAP: Record<string, string> = {
  美元: 'USD',
  人民币: 'CNY',
  欧元: 'EUR',
  日元: 'JPY',
  英镑: 'GBP',
  港币: 'HKD',
  韩元: 'KRW',
  台币: 'TWD',
}

const INPUT_RE = /^(\d+(?:\.\d+)?)\s*([A-Za-z]{3}|美元|人民币|欧元|日元|英镑|港币|韩元|台币)$/

export interface ParsedCurrencyInput {
  amount: number
  fromCurrency: string
}

/** 解析 "100 USD" / "100 美元" 形态输入；不匹配返回 null。 */
export function parseCurrencyInput(query: string): ParsedCurrencyInput | null {
  const m = query.trim().match(INPUT_RE)
  if (!m) return null
  const amount = parseFloat(m[1])
  const fromCurrency = CURRENCY_NAME_MAP[m[2]] ?? m[2].toUpperCase()
  return { amount, fromCurrency }
}

/** 以 USD 为基准的交叉汇率换算。 */
export function convertCurrency(
  amount: number,
  from: string,
  to: string,
  rates: Record<string, number>,
): number {
  const usdAmount = amount / rates[from]
  return usdAmount * rates[to]
}

/** 汇率缓存是否仍在 TTL 内（默认 10 分钟）。 */
export function isRatesCacheFresh(cacheTime: number, now: number, ttlMs = 10 * 60 * 1000): boolean {
  return now - cacheTime < ttlMs
}
