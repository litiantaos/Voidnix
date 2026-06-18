import { describe, it, expect } from 'vitest'

describe('time conversion logic', () => {
  it('Unix 秒时间戳 → 日期', () => {
    const ts = 1700000000
    const date = new Date(ts * 1000)
    expect(date.getFullYear()).toBe(2023)
  })

  it('Unix 毫秒时间戳 → 日期', () => {
    const ts = 1700000000000
    const date = new Date(ts)
    expect(date.getFullYear()).toBe(2023)
  })

  it('日期字符串 → Unix 时间戳', () => {
    const date = new Date('2024-01-01T00:00:00Z')
    const ts = Math.floor(date.getTime() / 1000)
    expect(ts).toBe(1704067200)
  })

  it('当前时间戳', () => {
    const now = Math.floor(Date.now() / 1000)
    expect(now).toBeGreaterThan(1700000000)
    expect(typeof now).toBe('number')
  })

  it('10 位数字识别为秒时间戳', () => {
    expect(/^\d{10}$/.test('1700000000')).toBe(true)
  })

  it('13 位数字识别为毫秒时间戳', () => {
    expect(/^\d{13}$/.test('1700000000000')).toBe(true)
  })
})
