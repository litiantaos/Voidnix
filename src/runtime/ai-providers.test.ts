import { describe, it, expect, beforeEach, vi } from 'vitest'
import {
  config,
  addAiProvider,
  removeAiProvider,
  removeKeyFromProvider,
  resolveCredentials,
  apiKeyOf,
  getKeySlot,
  addKeyToProvider,
  normalizeProvider,
  normalizeProvidersInPlace,
  parseSelectionKey,
  formatSelectionKey,
  modelSelectOptions,
  resolveUsageKind,
  onAiProvidersChange,
} from './ai-providers'

const mocks = vi.hoisted(() => {
  const memStore = new Map<string, unknown>()
  return {
    storeGet: vi.fn((k: string) => Promise.resolve(memStore.get(k))),
    storeSet: vi.fn((k: string, v: unknown) => {
      memStore.set(k, v)
      return Promise.resolve()
    }),
    storeSave: vi.fn(() => Promise.resolve()),
    storeClear: vi.fn(() => {
      memStore.clear()
      return Promise.resolve()
    }),
    storeOnChange: vi.fn(() => Promise.resolve(() => {})),
  }
})

vi.mock('@tauri-apps/plugin-store', () => ({
  load: () =>
    Promise.resolve({
      get: mocks.storeGet,
      set: mocks.storeSet,
      save: mocks.storeSave,
      clear: mocks.storeClear,
      onChange: mocks.storeOnChange,
    }),
}))

vi.mock('@/utils/tauri', () => ({ isTauri: false }))

beforeEach(() => {
  config.providers.splice(0, config.providers.length)
})

describe('multi-key', () => {
  it('add 默认一把 key，无 active 字段', () => {
    const id = addAiProvider({
      endpoint: 'https://x',
      apiKey: 'k',
      models: ['m'],
    })
    expect(config.providers[0].keys).toHaveLength(1)
    expect(apiKeyOf(config.providers[0])).toBe('k')
    expect(resolveCredentials({ providerId: id, model: 'm' })?.apiKey).toBe('k')
  })

  it('add 可指定 id；重复 id 幂等', () => {
    const id = addAiProvider({
      id: 'fixed-id',
      endpoint: 'https://x',
      apiKey: 'k',
      models: ['m'],
    })
    expect(id).toBe('fixed-id')
    const again = addAiProvider({
      id: 'fixed-id',
      endpoint: 'https://other',
      apiKey: 'other',
      models: ['z'],
    })
    expect(again).toBe('fixed-id')
    expect(config.providers).toHaveLength(1)
    expect(apiKeyOf(config.providers[0])).toBe('k')
  })

  it('addKey 后按 keyId 解析', () => {
    const id = addAiProvider({ endpoint: 'https://x', apiKey: 'a', models: ['m'] })
    const kid2 = addKeyToProvider(id, '备用')
    const p = config.providers[0]
    p.keys.find((k) => k.id === kid2)!.apiKey = 'b'
    expect(apiKeyOf(p, kid2)).toBe('b')
    expect(apiKeyOf(p)).toBe('a')
  })

  it('normalize legacy apiKey，剥掉 activeKeyId', () => {
    const n = normalizeProvider({
      id: 'old',
      endpoint: 'https://x',
      apiKey: 'legacy',
      models: ['m'],
      activeKeyId: 'whatever',
    })
    expect(n.keys[0].apiKey).toBe('legacy')
    expect('activeKeyId' in n).toBe(false)
  })

  it('resolveUsageKind 按 endpoint 识别', () => {
    const z = addAiProvider({ endpoint: 'https://open.bigmodel.cn/api/coding/paas/v4' })
    const d = addAiProvider({ endpoint: 'https://api.deepseek.com' })
    const o = addAiProvider({ endpoint: 'https://api.openai.com/v1' })
    expect(resolveUsageKind(config.providers.find((p) => p.id === z)!)).toBe('zhipu-coding-plan')
    expect(resolveUsageKind(config.providers.find((p) => p.id === d)!)).toBe('deepseek-balance')
    expect(resolveUsageKind(config.providers.find((p) => p.id === o)!)).toBe('')
  })
})

describe('resolveCredentials', () => {
  it('只按消费者传入的选用解析', () => {
    const id = addAiProvider({
      endpoint: 'https://cfg',
      models: ['cm'],
      keys: [
        { id: 'k1', label: '1', apiKey: 'ka' },
        { id: 'k2', label: '2', apiKey: 'kb' },
      ],
    })
    expect(resolveCredentials({ providerId: id, keyId: 'k2', model: 'cm' })?.apiKey).toBe('kb')
    expect(resolveCredentials({})).toBeNull()
  })

  it('remove provider', () => {
    const id = addAiProvider({ models: ['m'] })
    removeAiProvider(id)
    expect(config.providers).toHaveLength(0)
  })

  it('无 keyId 时优先第一把非空 Key', () => {
    const id = addAiProvider({
      endpoint: 'https://x',
      models: ['m'],
      keys: [
        { id: 'k1', label: '空', apiKey: '' },
        { id: 'k2', label: '好', apiKey: 'good' },
      ],
    })
    expect(apiKeyOf(config.providers.find((p) => p.id === id))).toBe('good')
    expect(getKeySlot(config.providers[0])?.id).toBe('k2')
  })

  it('normalizeProvidersInPlace 把 legacy apiKey 收成 keys', () => {
    config.providers.push({
      id: 'legacy',
      name: '',
      endpoint: 'https://x',
      models: ['m'],
      keys: undefined as unknown as [],
      usageKind: '',
      envKey: '',
      // @ts-expect-error legacy field
      apiKey: 'from-disk',
    })
    normalizeProvidersInPlace()
    expect(config.providers[0].keys[0].apiKey).toBe('from-disk')
  })

  it('remove 通知 onAiProvidersChange', () => {
    const events: string[] = []
    const off = onAiProvidersChange((e) => {
      events.push(e.kind)
    })
    const id = addAiProvider({
      endpoint: 'https://x',
      apiKey: 'a',
      models: ['m'],
      keys: [
        { id: 'k1', label: '1', apiKey: 'a' },
        { id: 'k2', label: '2', apiKey: 'b' },
      ],
    })
    removeKeyFromProvider(id, 'k2')
    removeAiProvider(id)
    expect(events).toEqual(['remove-key', 'remove-provider'])
    off()
  })
})

describe('selection key', () => {
  it('parse / format', () => {
    expect(parseSelectionKey('p::k::m')).toEqual({
      providerId: 'p',
      keyId: 'k',
      model: 'm',
    })
    expect(parseSelectionKey('p::m')).toEqual({ providerId: 'p', keyId: '', model: 'm' })
    expect(formatSelectionKey('p', 'k', 'm')).toBe('p::k::m')
  })

  it('modelSelectOptions 含 keyId', () => {
    const id = addAiProvider({
      endpoint: 'https://x',
      models: ['m1'],
      keys: [{ id: 'k1', label: '默认', apiKey: 'a' }],
    })
    const opts = modelSelectOptions() as { label: string; value: string }[]
    expect(opts[0].value).toBe(formatSelectionKey(id, 'k1', 'm1'))
  })
})
