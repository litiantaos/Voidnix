import { describe, it, expect } from 'vitest'
import { pad, toLocalIso, parseTimestamp } from './logic'

describe('pad', () => {
  it('单位数前补 0', () => {
    expect(pad(0)).toBe('00')
    expect(pad(5)).toBe('05')
    expect(pad(9)).toBe('09')
  })

  it('双位数原样返回', () => {
    expect(pad(10)).toBe('10')
    expect(pad(59)).toBe('59')
  })
})

describe('toLocalIso', () => {
  it('生成本地 ISO 字符串（含时区偏移）', () => {
    const iso = toLocalIso(new Date(0))
    expect(iso).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}[+-]\d{2}:\d{2}$/)
  })

  it('日期分量取自给定 Date', () => {
    const iso = toLocalIso(new Date('2024-06-20T12:00:00Z'))
    expect(iso.startsWith('2024-')).toBe(true)
  })
})

describe('parseTimestamp', () => {
  it('空输入返回 null', () => {
    expect(parseTimestamp('')).toBeNull()
    expect(parseTimestamp('   ')).toBeNull()
  })

  it('10 位秒时间戳 → isMs=false，ts 归一为毫秒', () => {
    expect(parseTimestamp('1700000000')).toEqual({ ts: 1700000000000, isMs: false })
  })

  it('13 位毫秒时间戳 → isMs=true', () => {
    expect(parseTimestamp('1700000000000')).toEqual({ ts: 1700000000000, isMs: true })
  })

  it('ISO 日期字符串 → isMs=true', () => {
    expect(parseTimestamp('2024-01-01T00:00:00Z')).toEqual({ ts: 1704067200000, isMs: true })
  })

  it('日期字符串（无时间）→ isMs=true', () => {
    expect(parseTimestamp('2024-01-01')).toEqual({ ts: 1704067200000, isMs: true })
  })

  it('无效输入返回 null', () => {
    expect(parseTimestamp('hello')).toBeNull()
    expect(parseTimestamp('not-a-date')).toBeNull()
  })

  it('边界：位数非 10/13 的纯数字按日期解析失败 → null', () => {
    expect(parseTimestamp('123456789')).toBeNull()
    expect(parseTimestamp('12345678901')).toBeNull()
    expect(parseTimestamp('12345678901234')).toBeNull()
  })
})
