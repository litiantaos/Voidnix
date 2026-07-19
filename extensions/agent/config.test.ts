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
  resolveAgentRuntimeCredentials,
} from './config'
import {
  config as aiProvidersConfig,
  addAiProvider,
  updateAiProvider,
  formatSelectionKey,
} from '@/runtime/ai-providers'
import * as aiHub from '@/runtime/ai-providers'

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
  it('读时失效视为未选；冷 prune 写回清空', () => {
    const id = addAiProvider({
      endpoint: 'https://x',
      models: ['m1', 'm2'],
      keys: [{ id: 'k1', label: '默认', apiKey: 'a' }],
    })
    config.providerModelKey = formatSelectionKey(id, 'k1', 'm1')
    expect(effectiveProviderModelKey.value).toBe(config.providerModelKey)

    updateAiProvider(id, { models: ['m2'] })
    // 热路径：不写回，effective 为空
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

describe('resolveAgentRuntimeCredentials', () => {
  it('无有效选用时走纯 env 路径', async () => {
    const spy = vi.spyOn(aiHub, 'resolveRuntimeCredentials').mockResolvedValue({
      endpoint: 'https://env',
      apiKey: 'ek',
      model: 'em',
      source: 'env',
    })
    config.providerModelKey = ''
    const r = await resolveAgentRuntimeCredentials()
    expect(spy).toHaveBeenCalledWith({})
    expect(r).toEqual({
      endpoint: 'https://env',
      apiKey: 'ek',
      model: 'em',
      source: 'env',
    })
  })

  it('悬空选用 effective 为空时同样走 env', async () => {
    config.providerModelKey = 'gone::k::m'
    const spy = vi.spyOn(aiHub, 'resolveRuntimeCredentials').mockResolvedValue({
      endpoint: 'https://env',
      apiKey: 'ek',
      model: 'em',
      source: 'env',
    })
    const r = await resolveAgentRuntimeCredentials()
    expect(effectiveProviderModelKey.value).toBe('')
    expect(spy).toHaveBeenCalledWith({})
    expect(r?.source).toBe('env')
  })

  it('有选用时按 provider/key/model 解析', async () => {
    const id = addAiProvider({
      endpoint: 'https://x',
      models: ['m1'],
      keys: [{ id: 'k1', label: '默认', apiKey: 'a' }],
    })
    config.providerModelKey = formatSelectionKey(id, 'k1', 'm1')
    const spy = vi.spyOn(aiHub, 'resolveRuntimeCredentials')
    const r = await resolveAgentRuntimeCredentials()
    expect(spy).toHaveBeenCalledWith({ providerId: id, keyId: 'k1', model: 'm1' })
    expect(r?.apiKey).toBe('a')
    expect(r?.source).toBe('config')
  })
})
