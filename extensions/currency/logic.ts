export const CURRENCIES = ['CNY', 'USD', 'EUR', 'JPY', 'GBP', 'HKD', 'KRW', 'TWD']

/** 中文名 → ISO 代码映射；未命中的代码原样大写。 */
const CURRENCY_NAME_MAP: Record<string, string> = {
  美元: 'USD',
  人民币: 'CNY',
  欧元: 'EUR',
  日元: 'JPY',
  英镑: 'GBP',
  港币: 'HKD',
  韩元: 'KRW',
  台币: 'TWD',
}

/** 量词 → 倍率（w=万、k=千；alternation 中万亿必须先于万匹配）。 */
const CURRENCY_UNITS: Record<string, number> = {
  万亿: 1e12,
  亿: 1e8,
  万: 1e4,
  w: 1e4,
  k: 1e3,
}

/** ISO 代码 → 中文名（CURRENCY_NAME_MAP 反向）。 */
export const CURRENCY_CODE_TO_NAME: Record<string, string> = Object.fromEntries(
  Object.entries(CURRENCY_NAME_MAP).map(([name, code]) => [code, name]),
)

// 公共片段：数字 / 量词 / 货币
const NUM = '\\d+(?:\\.\\d+)?'
const UNIT = '万亿|万|亿|w|k'
const CUR = '[A-Za-z]{3}|美元|人民币|欧元|日元|英镑|港币|韩元|台币'

/** 两种语序：数字[量词]货币 或 货币数字[量词]（量词总紧随数字）；i 标志使 w/k 兼容大小写。 */
const INPUT_RE = new RegExp(
  `^(${NUM})\\s*(${UNIT})?\\s*(${CUR})$|^(${CUR})\\s*(${NUM})\\s*(${UNIT})?$`,
  'i',
)

export interface ParsedCurrencyInput {
  amount: number
  fromCurrency: string
}

/** 解析 "100 USD" / "1万美元" / "3亿日元" / "USD10" / "美元10万" 形态输入；不匹配返回 null。 */
export function parseCurrencyInput(query: string): ParsedCurrencyInput | null {
  const m = query.trim().match(INPUT_RE)
  if (!m) return null
  // 形态一（数字在前）：[1]=数字 [2]=量词 [3]=货币
  // 形态二（货币在前）：[4]=货币 [5]=数字 [6]=量词
  const form1 = m[1] !== undefined
  const baseStr = form1 ? m[1] : m[5]
  const unitStr = form1 ? m[2] : m[6]
  const curStr = form1 ? m[3] : m[4]
  const base = parseFloat(baseStr!)
  const amount = base * (unitStr ? CURRENCY_UNITS[unitStr.toLowerCase()] : 1)
  const fromCurrency = CURRENCY_NAME_MAP[curStr!] ?? curStr!.toUpperCase()
  return { amount, fromCurrency }
}

/** 大数字按中文量词格式化：≥1万亿 转"万亿"、≥1亿 转亿、≥1万 转万，否则两位小数。 */
export function formatWithChineseUnit(n: number): string {
  const abs = Math.abs(n)
  if (abs >= 1e12) return (n / 1e12).toFixed(2) + '万亿'
  if (abs >= 1e8) return (n / 1e8).toFixed(2) + '亿'
  if (abs >= 1e4) return (n / 1e4).toFixed(2) + '万'
  return n.toFixed(2)
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
