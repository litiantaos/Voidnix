import { describe, it, expect } from 'vitest'
import { recencyScore, toResult } from './logic'
import type { RawSearchResult } from '@/utils/tauri'

const NOW = new Date('2026-01-15T00:00:00Z').getTime()
const HOUR = 3600000

describe('recencyScore', () => {
  it('null/空 → 0', () => {
    expect(recencyScore(null, NOW)).toBe(0)
  })

  it('<1h → 300', () => {
    expect(recencyScore(new Date(NOW - 30 * 60 * 1000).toISOString(), NOW)).toBe(300)
    expect(recencyScore(new Date(NOW).toISOString(), NOW)).toBe(300) // 恰好 0h
  })

  it('<24h → 200', () => {
    expect(recencyScore(new Date(NOW - 2 * HOUR).toISOString(), NOW)).toBe(200)
    expect(recencyScore(new Date(NOW - 23 * HOUR).toISOString(), NOW)).toBe(200)
  })

  it('<168h(7天) → 100', () => {
    expect(recencyScore(new Date(NOW - 48 * HOUR).toISOString(), NOW)).toBe(100)
  })

  it('<720h(30天) → 50', () => {
    expect(recencyScore(new Date(NOW - 200 * HOUR).toISOString(), NOW)).toBe(50)
  })

  it('>=720h → 0', () => {
    expect(recencyScore(new Date(NOW - 1000 * HOUR).toISOString(), NOW)).toBe(0)
  })

  it('负值（未来时间）视作最近期 → 300', () => {
    expect(recencyScore(new Date(NOW + HOUR).toISOString(), NOW)).toBe(300)
  })
})

describe('toResult', () => {
  const baseRaw: RawSearchResult = {
    id: 'app-1',
    title: 'Safari',
    path: '/Applications/Safari.app',
    kind: 'application',
    icon: 'data:image/png;base64,xxx',
    last_used: '2026-01-10',
    score: null,
    use_count: 5,
    parent: null,
  }

  it('映射基础字段 + boost 透传', () => {
    const r = toResult(baseRaw, 100)
    expect(r.id).toBe('app-1')
    expect(r.title).toBe('Safari')
    expect(r.description).toBe('/Applications/Safari.app')
    expect(r.boost).toBe(100)
    expect(r.data?.kind).toBe('application')
    expect(r.data?.useCount).toBe(5)
    expect(r.data?.icon).toBe('data:image/png;base64,xxx')
  })

  it('kind 透传（含 folder）', () => {
    const r = toResult({ ...baseRaw, kind: 'folder' }, 0)
    expect(r.data?.kind).toBe('folder')
  })

  it('icon null → data.icon null + 顶层 icon undefined', () => {
    const r = toResult({ ...baseRaw, icon: null }, 0)
    expect(r.icon).toBeUndefined()
    expect(r.data?.icon).toBeNull()
  })

  it('null 字段回退默认值', () => {
    const r = toResult({ ...baseRaw, use_count: null, last_used: null, parent: null }, 0)
    expect(r.data?.useCount).toBe(0)
    expect(r.data?.lastUsed).toBeNull()
  })
})
