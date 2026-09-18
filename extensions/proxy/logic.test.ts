import { describe, it, expect } from 'vitest'
import { t } from '@/runtime/i18n'
import './locales'
import {
  delayColor,
  formatDelay,
  formatSubTime,
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

  it('formatSubTime：ISO UTC 转本地日期', () => {
    const pad = (n: number) => String(n).padStart(2, '0')
    // 本地正午时刻（本地分量定义），转 ISO 后经 formatSubTime 回读，任何时区下本地日期都是今天
    const localNoon = new Date()
    localNoon.setHours(12, 0, 0, 0)
    expect(formatSubTime(localNoon.toISOString())).toBe(
      `${localNoon.getFullYear()}-${pad(localNoon.getMonth() + 1)}-${pad(localNoon.getDate())}`,
    )
  })

  it('formatSubTime：空串 → 未更新文案，非法串原样返回', () => {
    expect(formatSubTime('')).toBe(t('proxy.notUpdated'))
    expect(formatSubTime('garbage')).toBe('garbage')
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
