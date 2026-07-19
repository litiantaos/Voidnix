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
  pruneAiSelections,
  effectiveAiSelections,
} from './config'
import {
  config as aiProvidersConfig,
  addAiProvider,
  updateAiProvider,
  removeAiProvider,
  removeKeyFromProvider,
} from '@/runtime/ai-providers'

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
  it('读写 selections（写入压滤无效项）', () => {
    addAiProvider({
      id: 'p1',
      endpoint: 'https://x',
      models: ['m1'],
      keys: [{ id: 'k', label: '默认', apiKey: 'a' }],
    })
    updateAiConfig({
      selections: [
        { providerId: 'p1', model: 'm1' },
        { providerId: 'gone', model: 'x' },
      ],
      prompt: 'hi',
    })
    const ai = getAiConfig()
    expect(ai.selections).toEqual([{ providerId: 'p1', keyId: 'k', model: 'm1' }])
    expect(ai.prompt).toBe('hi')
  })
})

describe('resolveAiTargets', () => {
  it('按 selections 跨提供商解析；单 Key label 不带备注', () => {
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
    expect(targets.map((t) => t.label)).toEqual(['A', 'B'])
  })

  it('按 keyId 解析多 Key；label 带备注', () => {
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
    expect(targets[0].label).toBe('A · 备')
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

  it('中枢已删的模型不解析', () => {
    aiProvidersConfig.providers.push({
      id: 'p1',
      name: 'A',
      endpoint: 'https://1',
      models: ['m1'],
      keys: [{ id: 'k', label: '默认', apiKey: 'k1' }],
      usageKind: '',
      envKey: '',
    })
    const targets = resolveAiTargets({
      id: SERVICE_AI_ID,
      type: 'ai',
      selections: [
        { providerId: 'p1', model: 'm1' },
        { providerId: 'p1', model: 'stale' },
      ],
      prompt: '',
    })
    expect(targets).toHaveLength(1)
    expect(targets[0].model).toBe('m1')
  })
})

describe('effectiveAiSelections / prune', () => {
  it('读时过滤悬空；冷 prune / updateAiConfig 写回干净', () => {
    const id = addAiProvider({
      endpoint: 'https://x',
      models: ['m1', 'm2'],
      keys: [
        { id: 'k1', label: '主', apiKey: 'a' },
        { id: 'k2', label: '备', apiKey: 'b' },
      ],
    })
    // 直接写盘字段（绕过 update 压滤）模拟脏数据
    getAiConfig().selections = [
      { providerId: id, keyId: 'k1', model: 'm1' },
      { providerId: id, keyId: 'k2', model: 'm2' },
      { providerId: 'gone', model: 'm1' },
    ]
    updateAiProvider(id, { models: ['m1'] })

    expect(effectiveAiSelections(getAiConfig().selections)).toEqual([
      { providerId: id, keyId: 'k1', model: 'm1' },
    ])
    // 热路径不写回
    expect(getAiConfig().selections).toHaveLength(3)

    pruneAiSelections()
    expect(getAiConfig().selections).toEqual([{ providerId: id, keyId: 'k1', model: 'm1' }])

    removeKeyFromProvider(id, 'k1')
    expect(effectiveAiSelections(getAiConfig().selections)).toEqual([])
    pruneAiSelections()
    expect(getAiConfig().selections).toEqual([])

    // updateAiConfig 写入时压滤
    getAiConfig().selections = [{ providerId: 'gone', model: 'x' }]
    updateAiConfig({ selections: [{ providerId: 'gone', model: 'x' }] })
    expect(getAiConfig().selections).toEqual([])

    removeAiProvider(id)
  })

  it('旧式无 keyId 与带 keyId 的同一模型去重并补全 keyId', () => {
    const id = addAiProvider({
      endpoint: 'https://api.deepseek.com',
      models: ['deepseek-v4-flash', 'deepseek-v4-pro'],
      keys: [{ id: 'k-default', label: '默认', apiKey: 'sk' }],
    })
    const z = addAiProvider({
      endpoint: 'https://open.bigmodel.cn/x',
      models: ['glm-5.2'],
      keys: [{ id: 'kz', label: '195', apiKey: 'zk' }],
    })
    // 复现线上脏数据：flash 同时有 legacy + 三段式
    getAiConfig().selections = [
      { providerId: id, model: 'deepseek-v4-flash' },
      { providerId: z, model: 'glm-5.2' },
      { providerId: id, keyId: 'k-default', model: 'deepseek-v4-flash' },
    ]
    const eff = effectiveAiSelections(getAiConfig().selections)
    expect(eff).toEqual([
      { providerId: id, keyId: 'k-default', model: 'deepseek-v4-flash' },
      { providerId: z, keyId: 'kz', model: 'glm-5.2' },
    ])
    expect(resolveAiTargets(getAiConfig()).map((t) => t.model)).toEqual([
      'deepseek-v4-flash',
      'glm-5.2',
    ])

    pruneAiSelections()
    expect(getAiConfig().selections).toEqual(eff)
  })
})
