import { describe, it, expect, beforeEach, vi } from 'vitest'

vi.mock('@tauri-apps/plugin-store', () => ({
  load: () =>
    Promise.resolve({
      get: () => Promise.resolve(undefined),
      set: () => Promise.resolve(),
      save: () => Promise.resolve(),
      clear: () => Promise.resolve(),
      onChange: () => Promise.resolve(() => {}),
    }),
}))

vi.mock('@/utils/tauri', () => ({ isTauri: false }))

import {
  config,
  resolveAiTargets,
  SERVICE_YOUDAO_ID,
  SERVICE_AI_ID,
  getAiConfig,
  updateAiConfig,
} from './config'
import { config as aiProvidersConfig } from '@/runtime/ai-providers'

beforeEach(() => {
  aiProvidersConfig.providers.splice(0, aiProvidersConfig.providers.length)
  config.configs.splice(
    0,
    config.configs.length,
    { id: SERVICE_YOUDAO_ID, type: 'youdao', appKey: '', appSecret: '' },
    { id: SERVICE_AI_ID, type: 'ai', selections: [], prompt: '' },
  )
})

describe('getAiConfig / updateAiConfig', () => {
  it('读写 selections', () => {
    updateAiConfig({
      selections: [{ providerId: 'p1', model: 'm1' }],
      prompt: 'hi',
    })
    const ai = getAiConfig()
    expect(ai.selections).toEqual([{ providerId: 'p1', model: 'm1' }])
    expect(ai.prompt).toBe('hi')
  })
})

describe('resolveAiTargets', () => {
  it('按 selections 跨提供商解析', () => {
    aiProvidersConfig.providers.push(
      {
        id: 'p1',
        name: 'A',
        endpoint: 'https://1',
        models: ['m1'],
        keys: [{ id: 'k', label: '默认', apiKey: 'k1' }],
        usageKind: '',
        envKey: '',
      },
      {
        id: 'p2',
        name: 'B',
        endpoint: 'https://2',
        models: ['m2'],
        keys: [{ id: 'k', label: '默认', apiKey: 'k2' }],
        usageKind: '',
        envKey: '',
      },
    )
    const targets = resolveAiTargets({
      id: SERVICE_AI_ID,
      type: 'ai',
      selections: [
        { providerId: 'p1', model: 'm1' },
        { providerId: 'p2', model: 'm2' },
      ],
      prompt: '',
    })
    expect(targets).toHaveLength(2)
    expect(targets.map((t) => t.apiKey)).toEqual(['k1', 'k2'])
  })

  it('按 keyId 解析多 Key', () => {
    aiProvidersConfig.providers.push({
      id: 'p1',
      name: 'A',
      endpoint: 'https://1',
      models: ['m1'],
      keys: [
        { id: 'k1', label: '主', apiKey: 'a' },
        { id: 'k2', label: '备', apiKey: 'b' },
      ],
      usageKind: '',
      envKey: '',
    })
    const targets = resolveAiTargets({
      id: SERVICE_AI_ID,
      type: 'ai',
      selections: [{ providerId: 'p1', keyId: 'k2', model: 'm1' }],
      prompt: '',
    })
    expect(targets).toHaveLength(1)
    expect(targets[0].apiKey).toBe('b')
    expect(targets[0].label).toContain('备')
  })

  it('第一把空时回退非空 Key', () => {
    aiProvidersConfig.providers.push({
      id: 'p1',
      name: 'A',
      endpoint: 'https://1',
      models: ['m1'],
      keys: [
        { id: 'k1', label: '空', apiKey: '' },
        { id: 'k2', label: '好', apiKey: 'good' },
      ],
      usageKind: '',
      envKey: '',
    })
    const targets = resolveAiTargets({
      id: SERVICE_AI_ID,
      type: 'ai',
      selections: [{ providerId: 'p1', model: 'm1' }],
      prompt: '',
    })
    expect(targets[0]?.apiKey).toBe('good')
  })

  it('selections 缺失时不抛错', () => {
    expect(
      resolveAiTargets({
        id: SERVICE_AI_ID,
        type: 'ai',
        selections: undefined as unknown as [],
        prompt: '',
      }),
    ).toEqual([])
  })
})
