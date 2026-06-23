import { describe, it, expect } from 'vitest'
import {
  parseCurrencyInput,
  convertCurrency,
  isRatesCacheFresh,
  formatWithChineseUnit,
  CURRENCIES,
} from './logic'

describe('parseCurrencyInput', () => {
  it('解析英文代码', () => {
    expect(parseCurrencyInput('100 USD')).toEqual({ amount: 100, fromCurrency: 'USD' })
    expect(parseCurrencyInput('12.5 EUR')).toEqual({ amount: 12.5, fromCurrency: 'EUR' })
  })

  it('解析中文币名', () => {
    expect(parseCurrencyInput('100 美元')).toEqual({ amount: 100, fromCurrency: 'USD' })
    expect(parseCurrencyInput('500人民币')).toEqual({ amount: 500, fromCurrency: 'CNY' })
  })

  it('小写代码大写化', () => {
    expect(parseCurrencyInput('100 jpy')).toEqual({ amount: 100, fromCurrency: 'JPY' })
  })

  it('解析中文量词（万/亿/万亿）', () => {
    expect(parseCurrencyInput('1万美元')).toEqual({ amount: 10000, fromCurrency: 'USD' })
    expect(parseCurrencyInput('1.5亿 CNY')).toEqual({ amount: 150000000, fromCurrency: 'CNY' })
    expect(parseCurrencyInput('3万日元')).toEqual({ amount: 30000, fromCurrency: 'JPY' })
    expect(parseCurrencyInput('2万亿USD')).toEqual({ amount: 2e12, fromCurrency: 'USD' })
  })

  it('量词与货币间容忍空白', () => {
    expect(parseCurrencyInput('1 万 USD')).toEqual({ amount: 10000, fromCurrency: 'USD' })
  })

  it('万亿优先于万匹配（不残留"亿"）', () => {
    expect(parseCurrencyInput('5万亿美元')).toEqual({ amount: 5e12, fromCurrency: 'USD' })
  })

  it('拒绝非法输入', () => {
    expect(parseCurrencyInput('')).toBeNull()
    expect(parseCurrencyInput('100')).toBeNull()
    expect(parseCurrencyInput('abc')).toBeNull()
    expect(parseCurrencyInput('100 USDT')).toBeNull() // 4 字母
    expect(parseCurrencyInput('100 元')).toBeNull()
  })

  it('trim 前后空白', () => {
    expect(parseCurrencyInput('  100 USD  ')).toEqual({ amount: 100, fromCurrency: 'USD' })
  })
})

describe('formatWithChineseUnit', () => {
  it('小于1万保留两位小数', () => {
    expect(formatWithChineseUnit(1234.56)).toBe('1234.56')
    expect(formatWithChineseUnit(0.91)).toBe('0.91')
  })

  it('大于等于1万转万', () => {
    expect(formatWithChineseUnit(123456)).toBe('12.35万')
    expect(formatWithChineseUnit(10000)).toBe('1.00万')
  })

  it('大于等于1亿转亿', () => {
    expect(formatWithChineseUnit(123456789)).toBe('1.23亿')
    expect(formatWithChineseUnit(100000000)).toBe('1.00亿')
  })

  it('大于等于1万亿转万亿', () => {
    expect(formatWithChineseUnit(1.5e12)).toBe('1.50万亿')
    expect(formatWithChineseUnit(1e12)).toBe('1.00万亿')
  })

  it('负数按绝对值判级', () => {
    expect(formatWithChineseUnit(-123456)).toBe('-12.35万')
  })
})

describe('convertCurrency', () => {
  const rates = { USD: 1, CNY: 7, EUR: 0.9 }

  it('USD → CNY', () => {
    expect(convertCurrency(100, 'USD', 'CNY', rates)).toBeCloseTo(700)
  })

  it('CNY → EUR（交叉汇率）', () => {
    // 100 CNY = 100/7 USD ≈ 14.286 USD × 0.9 ≈ 12.857 EUR
    expect(convertCurrency(100, 'CNY', 'EUR', rates)).toBeCloseTo((100 / 7) * 0.9)
  })
})

describe('isRatesCacheFresh', () => {
  it('TTL 内为新鲜', () => {
    expect(isRatesCacheFresh(1000, 2000)).toBe(true) // 1s < 10min
  })

  it('超过 TTL 不新鲜', () => {
    const ttl = 10 * 60 * 1000
    expect(isRatesCacheFresh(1000, 1000 + ttl)).toBe(false)
    expect(isRatesCacheFresh(1000, 1000 + ttl + 1)).toBe(false)
  })

  it('自定义 TTL', () => {
    expect(isRatesCacheFresh(0, 500, 1000)).toBe(true)
    expect(isRatesCacheFresh(0, 1001, 1000)).toBe(false)
  })
})

describe('CURRENCIES', () => {
  it('包含主要货币且 USD 为基准', () => {
    expect(CURRENCIES).toContain('USD')
    expect(CURRENCIES).toContain('CNY')
    expect(CURRENCIES.length).toBeGreaterThan(4)
  })
})
