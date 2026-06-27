import { describe, it, expect } from 'vitest'
import {
  modeLabel,
  delayColor,
  formatDelay,
  filterNodes,
  isUserSelectorGroup,
  pickMainGroup,
  latestDelay,
  type ProxyEntry,
} from './logic'

describe('proxy logic', () => {
  it('modeLabel maps known modes', () => {
    expect(modeLabel('rule')).toBe('规则')
    expect(modeLabel('global')).toBe('全局')
    expect(modeLabel('direct')).toBe('直连')
    expect(modeLabel('unknown')).toBe('unknown')
  })

  it('delayColor classifies latency buckets', () => {
    expect(delayColor(null)).toBe('text-tx-muted')
    expect(delayColor(0)).toBe('text-tx-muted')
    expect(delayColor(100)).toBe('text-green-500')
    expect(delayColor(150)).toBe('text-yellow-500')
    expect(delayColor(400)).toBe('text-red-500')
  })

  it('formatDelay renders latency', () => {
    expect(formatDelay(null)).toBe('')
    expect(formatDelay(0)).toBe('')
    expect(formatDelay(123)).toBe('123ms')
  })

  it('filterNodes matches case-insensitive substring', () => {
    const nodes = [{ name: 'HK-01' }, { name: 'US-02' }, { name: 'JP Premium' }]
    expect(filterNodes(nodes, 'hk')).toEqual([{ name: 'HK-01' }])
    expect(filterNodes(nodes, '')).toHaveLength(3)
    expect(filterNodes(nodes, 'premium')).toEqual([{ name: 'JP Premium' }])
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

  it('pickMainGroup prefers user selector over GLOBAL', () => {
    const proxies = {
      GLOBAL: selector('GLOBAL', ['a'], 'a'),
      节点选择: selector('节点选择', ['DIRECT', 'HK-1'], 'HK-1'),
      'HK-1': node('HK-1'),
    }
    const g = pickMainGroup(proxies)
    expect(g?.name).toBe('节点选择')
  })

  it('pickMainGroup falls back to GLOBAL when no user selector', () => {
    const proxies = { GLOBAL: selector('GLOBAL', ['a'], 'a'), a: node('a') }
    expect(pickMainGroup(proxies)?.name).toBe('GLOBAL')
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
