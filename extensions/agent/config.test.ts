import { describe, it, expect, beforeEach, vi } from 'vitest'

vi.mock('@tauri-apps/plugin-store', () => ({
  load: () =>
    Promise.resolve({
      get: () => Promise.resolve(undefined),
      set: () => Promise.resolve(),
      save: () => Promise.resolve(),
      clear: () => Promise.resolve(),
      delete: () => Promise.resolve(true),
      onChange: () => Promise.resolve(() => {}),
    }),
}))

vi.mock('@/utils/tauri', () => ({ isTauri: false }))

import {
  config,
  updateSearchProvider,
  pruneAgentSelection,
  effectiveProviderModelKey,
  setProviderModelKey,
  resolveAgentCredentials,
} from './config'
import {
  config as aiProvidersConfig,
  addAiProvider,
  removeAiProvider,
  formatSelectionKey,
} from '@/runtime/ai-providers'

beforeEach(() => {
  aiProvidersConfig.providers.splice(0, aiProvidersConfig.providers.length)
  config.searchProvider.apiKey = ''
  config.systemPrompt = 'test'
  config.providerModelKey = ''
  vi.restoreAllMocks()
})

describe('updateSearchProvider', () => {
  it('更新 tavily key', () => {
    updateSearchProvider({ apiKey: 'tvly-x' })
    expect(config.searchProvider.apiKey).toBe('tvly-x')
  })
})

describe('effectiveProviderModelKey / prune', () => {
  it('显式选用合法时规范三段', () => {
    const id = addAiProvider({
      endpoint: 'https://x',
      models: ['m1', 'm2'],
      keys: [{ id: 'k1', label: '默认', apiKey: 'a' }],
    })
    config.providerModelKey = formatSelectionKey(id, 'k1', 'm1')
    expect(effectiveProviderModelKey.value).toBe(config.providerModelKey)
  })

  it('无显式选用时默认首个可用提供商', () => {
    const id = addAiProvider({
      endpoint: 'https://x',
      models: ['m1', 'm2'],
      keys: [{ id: 'k1', label: '默认', apiKey: 'a' }],
    })
    config.providerModelKey = ''
    expect(effectiveProviderModelKey.value).toBe(formatSelectionKey(id, 'k1', 'm1'))
  })

  it('首个提供商缺 key / 模型时落到下一个可用提供商', () => {
    addAiProvider({
      endpoint: 'https://empty',
      models: ['m1'],
      keys: [{ id: 'ke', label: '空', apiKey: '' }],
    })
    const id2 = addAiProvider({
      endpoint: 'https://ok',
      models: ['m1'],
      keys: [{ id: 'k2', label: '默认', apiKey: 'a' }],
    })
    config.providerModelKey = ''
    expect(effectiveProviderModelKey.value).toBe(formatSelectionKey(id2, 'k2', 'm1'))
  })

  it('显式选用悬空且无其他可用提供商时 effective 为空；冷 prune 清空持久值', () => {
    const id = addAiProvider({
      endpoint: 'https://x',
      models: ['m1'],
      keys: [{ id: 'k1', label: '默认', apiKey: 'a' }],
    })
    config.providerModelKey = formatSelectionKey(id, 'k1', 'm1')
    removeAiProvider(id)
    // 热路径：不写回，effective 回退无可用 → 空
    expect(config.providerModelKey).toContain('m1')
    expect(effectiveProviderModelKey.value).toBe('')

    pruneAgentSelection()
    expect(config.providerModelKey).toBe('')
  })

  it('旧式两段串 effective 规范为三段；冷 prune 写回', () => {
    const id = addAiProvider({
      endpoint: 'https://x',
      models: ['m1'],
      keys: [{ id: 'k1', label: '默认', apiKey: 'a' }],
    })
    config.providerModelKey = `${id}::m1`
    expect(effectiveProviderModelKey.value).toBe(formatSelectionKey(id, 'k1', 'm1'))
    // 热路径不写回
    expect(config.providerModelKey).toBe(`${id}::m1`)

    pruneAgentSelection()
    expect(config.providerModelKey).toBe(formatSelectionKey(id, 'k1', 'm1'))
  })

  it('setProviderModelKey 无效串忽略并 warn', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
    setProviderModelKey('gone::k::m')
    expect(config.providerModelKey).toBe('')
    expect(warn).toHaveBeenCalled()
  })
})

describe('resolveAgentCredentials', () => {
  it('中枢无可用提供商时返回 null', () => {
    config.providerModelKey = ''
    expect(resolveAgentCredentials()).toBeNull()
  })

  it('无显式选用时按首个可用提供商解析', () => {
    const id = addAiProvider({
      endpoint: 'https://x',
      models: ['m1'],
      keys: [{ id: 'k1', label: '默认', apiKey: 'a' }],
    })
    config.providerModelKey = ''
    const r = resolveAgentCredentials()
    expect(r?.apiKey).toBe('a')
    expect(r?.endpoint).toBe('https://x')
    expect(r?.model).toBe('m1')
    expect(r?.source).toBe('config')
    expect(r?.providerId).toBe(id)
  })

  it('显式选用时按选用解析（非默认首个）', () => {
    addAiProvider({
      endpoint: 'https://first',
      models: ['mf'],
      keys: [{ id: 'kf', label: '默认', apiKey: 'first' }],
    })
    const id2 = addAiProvider({
      endpoint: 'https://second',
      models: ['m1'],
      keys: [{ id: 'k2', label: '默认', apiKey: 'a' }],
    })
    config.providerModelKey = formatSelectionKey(id2, 'k2', 'm1')
    const r = resolveAgentCredentials()
    expect(r?.endpoint).toBe('https://second')
    expect(r?.apiKey).toBe('a')
  })

  it('悬空选用且无其他可用提供商时返回 null', () => {
    const id = addAiProvider({
      endpoint: 'https://x',
      models: ['m1'],
      keys: [{ id: 'k1', label: '默认', apiKey: 'a' }],
    })
    config.providerModelKey = formatSelectionKey(id, 'k1', 'm1')
    removeAiProvider(id)
    expect(resolveAgentCredentials()).toBeNull()
  })
})
