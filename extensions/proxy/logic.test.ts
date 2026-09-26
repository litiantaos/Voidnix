import { describe, it, expect } from 'vitest'
import './locales'
import {
  delayColor,
  formatDelay,
  formatSubExpiry,
  isSubExpired,
  DELAY_TIMEOUT,
  filterNodes,
  isUserSelectorGroup,
  latestDelay,
  type ProxyEntry,
} from './logic'

describe('proxy logic', () => {
  it('delayColor连通绿/超时红/未测速空', () => {
    expect(delayColor(DELAY_TIMEOUT)).toBe('text-danger')
    expect(delayColor(100)).toBe('text-success')
    expect(delayColor(500)).toBe('text-success')
    expect(delayColor(0)).toBe('')
  })

  it('formatDelay renders latency', () => {
    expect(formatDelay(null)).toBe('')
    expect(formatDelay(0)).toBe('')
    expect(formatDelay(DELAY_TIMEOUT)).toBe('超时')
    expect(formatDelay(123)).toBe('123ms')
  })

  it('filterNodes matches case-insensitive substring', () => {
    const nodes = [{ name: 'HK-01' }, { name: 'US-02' }, { name: 'JP Premium' }]
    expect(filterNodes(nodes, 'hk')).toEqual([{ name: 'HK-01' }])
    expect(filterNodes(nodes, '')).toHaveLength(3)
    expect(filterNodes(nodes, 'premium')).toEqual([{ name: 'JP Premium' }])
  })

  it('formatSubExpiry：ISO UTC 转本地日期', () => {
    const pad = (n: number) => String(n).padStart(2, '0')
    // 本地正午时刻（本地分量定义），转 ISO 后经 formatSubExpiry 回读，任何时区下本地日期都是今天
    const localNoon = new Date()
    localNoon.setHours(12, 0, 0, 0)
    expect(formatSubExpiry(localNoon.toISOString())).toBe(
      `${localNoon.getFullYear()}-${pad(localNoon.getMonth() + 1)}-${pad(localNoon.getDate())}`,
    )
  })

  it('formatSubExpiry：空/未提供/非法串 → 空串（调用方省略该段）', () => {
    expect(formatSubExpiry('')).toBe('')
    expect(formatSubExpiry(undefined)).toBe('')
    expect(formatSubExpiry('garbage')).toBe('')
  })

  it('isSubExpired：过期判定', () => {
    expect(isSubExpired(new Date(Date.now() - 86400_000).toISOString())).toBe(true)
    expect(isSubExpired(new Date(Date.now() + 86400_000).toISOString())).toBe(false)
    // 未提供/非法：不判过期（无信息不作危险断言）
    expect(isSubExpired('')).toBe(false)
    expect(isSubExpired(undefined)).toBe(false)
    expect(isSubExpired('garbage')).toBe(false)
  })
})

describe('proxy group helpers', () => {
  const selector = (name: string, all: string[], now: string): ProxyEntry => ({
    name,
    type: 'Selector',
    all,
    now,
  })
  const node = (name: string, delay = 0): ProxyEntry => ({
    name,
    type: 'ss',
    history: delay > 0 ? [{ time: '', delay }] : [],
  })

  it('isUserSelectorGroup excludes GLOBAL', () => {
    expect(isUserSelectorGroup(selector('节点选择', ['DIRECT'], 'DIRECT'))).toBe(true)
    expect(isUserSelectorGroup({ ...selector('GLOBAL', ['a'], 'a') })).toBe(false)
    expect(isUserSelectorGroup(node('HK-1'))).toBe(false)
  })

  it('latestDelay reads last history entry', () => {
    expect(
      latestDelay([
        { time: '1', delay: 100 },
        { time: '2', delay: 200 },
      ]),
    ).toBe(200)
    expect(latestDelay([])).toBe(0)
    expect(latestDelay(undefined)).toBe(0)
  })
})
