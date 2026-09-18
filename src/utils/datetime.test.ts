import { describe, it, expect } from 'vitest'
import { parseUtcMs } from './datetime'

describe('parseUtcMs', () => {
  it('SQLite datetime 格式（无偏移段）按 UTC 解析', () => {
    expect(parseUtcMs('2024-01-15 12:30:45')).toBe(Date.UTC(2024, 0, 15, 12, 30, 45))
  })

  it('mdls/mdfind 格式（+0000）按显式偏移解析', () => {
    expect(parseUtcMs('2024-01-15 12:30:45 +0000')).toBe(Date.UTC(2024, 0, 15, 12, 30, 45))
  })

  it('非零偏移（+0800）按偏移折算', () => {
    expect(parseUtcMs('2024-01-15 12:30:45 +0800')).toBe(Date.UTC(2024, 0, 15, 4, 30, 45))
  })

  it('带冒号偏移（+05:30）按偏移折算', () => {
    expect(parseUtcMs('2024-01-15 12:30:45 +05:30')).toBe(Date.UTC(2024, 0, 15, 7, 0, 45))
  })

  it('标准 ISO（Z / 毫秒）原样解析', () => {
    expect(parseUtcMs('2024-01-15T12:30:45Z')).toBe(Date.UTC(2024, 0, 15, 12, 30, 45))
    expect(parseUtcMs('2024-01-15T12:30:45.123Z')).toBe(Date.UTC(2024, 0, 15, 12, 30, 45, 123))
  })

  it('纯日期串回退原生解析（规范语义：UTC 午夜）', () => {
    expect(parseUtcMs('2024-01-15')).toBe(Date.UTC(2024, 0, 15))
  })

  it('非法输入返回 NaN', () => {
    expect(parseUtcMs('hello')).toBeNaN()
    expect(parseUtcMs('')).toBeNaN()
  })
})
