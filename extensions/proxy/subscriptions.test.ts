import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { nextTick } from 'vue'

// 订阅数据层：自动更新过期判定 / 激活订阅摘要 / 配置不变式。
// isTauri=false 跳过模块级自动更新轮询；plugin-store 不可达时 defineConfig 纯内存。
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/plugin-store', () => ({
  load: vi.fn().mockRejectedValue(new Error('no store')),
}))
vi.mock('@/utils/tauri', () => ({ isTauri: false }))

import { config } from './config'
import { activeSubscription, isSubscriptionStale } from './subscriptions'

function sub(partial: Partial<{ id: string; name: string; url: string; proxyCount: number }> = {}) {
  return {
    id: partial.id ?? 'a',
    name: partial.name ?? '',
    url: partial.url ?? 'https://s.example/sub',
    updatedAt: '',
    expiresAt: '',
    proxyCount: partial.proxyCount ?? 1,
  }
}

describe('isSubscriptionStale 自动更新过期判定', () => {
  const HOUR = 3600_000
  const ago = (h: number) => new Date(Date.now() - h * HOUR).toISOString()

  beforeEach(() => {
    config.autoUpdateEnabled = true
    config.autoUpdateIntervalHours = 24
  })
  afterEach(() => {
    config.autoUpdateIntervalHours = 24
  })

  it('无 url / 间隔内拉取过 → 不过期；从未拉取（updatedAt 空）→ 过期', () => {
    expect(isSubscriptionStale({ ...sub(), url: '', updatedAt: ago(48) })).toBe(false)
    expect(isSubscriptionStale({ ...sub(), updatedAt: ago(23), proxyCount: 1 })).toBe(false)
    expect(isSubscriptionStale({ ...sub(), updatedAt: '', proxyCount: 0 })).toBe(true)
  })

  it('超过间隔 → 过期；间隔设置即时生效', () => {
    const s = { ...sub(), updatedAt: ago(3) }
    expect(isSubscriptionStale(s)).toBe(false) // 3h < 24h
    config.autoUpdateIntervalHours = 1
    expect(isSubscriptionStale(s)).toBe(true) // 3h > 1h
  })

  it('间隔非法值防御：0/负数钳到 1h，NaN 落 24h 档', () => {
    const s = { ...sub(), updatedAt: ago(2) }
    config.autoUpdateIntervalHours = 0
    expect(isSubscriptionStale(s)).toBe(true) // 2h > 钳后 1h
    expect(isSubscriptionStale({ ...s, updatedAt: ago(0.5) })).toBe(false)
    config.autoUpdateIntervalHours = Number.NaN
    expect(isSubscriptionStale({ ...s, updatedAt: ago(25) })).toBe(true) // NaN → 24h 档
    expect(isSubscriptionStale({ ...s, updatedAt: ago(23) })).toBe(false)
  })
})

describe('激活订阅摘要与配置不变式', () => {
  beforeEach(() => {
    config.subscriptions = [sub({ id: 'a', name: 'A' }), sub({ id: 'b', name: 'B' })]
    config.activeSubscriptionId = 'b'
  })

  it('摘要跟随 activeSubscriptionId；失效时归一化回首项', async () => {
    expect(activeSubscription.value?.id).toBe('b')

    config.activeSubscriptionId = 'missing'
    await nextTick()
    expect(config.activeSubscriptionId).toBe('a')
    expect(activeSubscription.value?.id).toBe('a')
  })

  it('订阅列表为空时自动补默认项并激活', async () => {
    config.subscriptions = []
    await nextTick()
    await nextTick()
    expect(config.subscriptions).toHaveLength(1)
    expect(config.activeSubscriptionId).toBe(config.subscriptions[0]!.id)
    expect(activeSubscription.value?.id).toBe(config.activeSubscriptionId)
  })
})
