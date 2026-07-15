import { describe, it, expect, beforeEach, vi } from 'vitest'
import {
  config,
  activeProviderConfig,
  addAiProvider,
  removeAiProvider,
  updateAiProvider,
  setActiveProviderModelKey,
  resolveActiveModel,
  isProviderReady,
} from './config'

// config.ts 顶层立即调用 defineConfig（触发 plugin-store load），mock 变量须经 vi.hoisted 提升以避开 TDZ
const mocks = vi.hoisted(() => {
  const memStore = new Map<string, unknown>()
  return {
    memStore,
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

// defineConfig 返回模块级单例 reactive，测试间共享；beforeEach 显式重置 aiProviders 至已知状态
beforeEach(() => {
  config.aiProviders.splice(0, config.aiProviders.length, {
    id: 'p1',
    endpoint: '',
    apiKey: '',
    models: ['m1'],
  })
  config.activeProviderModelKey = ''
})

describe('resolveActiveModel / isProviderReady', () => {
  it('无 :: 时 model 为空且未就绪', () => {
    expect(resolveActiveModel()).toBe('')
    expect(isProviderReady.value).toBe(false)
  })

  it('endpoint + apiKey + model 齐全时就绪', () => {
    config.aiProviders[0].endpoint = 'https://api.openai.com/v1'
    config.aiProviders[0].apiKey = 'sk-test'
    setActiveProviderModelKey('p1::m1')
    expect(resolveActiveModel()).toBe('m1')
    expect(isProviderReady.value).toBe(true)
  })

  it('缺 model 时不就绪', () => {
    config.aiProviders[0].endpoint = 'https://api.openai.com/v1'
    config.aiProviders[0].apiKey = 'sk-test'
    setActiveProviderModelKey('p1::')
    expect(resolveActiveModel()).toBe('')
    expect(isProviderReady.value).toBe(false)
  })
})

describe('activeProviderConfig', () => {
  it('默认返回第一个', () => {
    expect(activeProviderConfig.value.id).toBe('p1')
  })

  it('ID:: 前缀匹配指定配置', () => {
    setActiveProviderModelKey('p1::')
    expect(activeProviderConfig.value.id).toBe('p1')
  })

  it('未匹配时回退到第一个', () => {
    setActiveProviderModelKey('nonexistent::')
    expect(activeProviderConfig.value.id).toBe('p1')
  })
})

describe('addAiProvider', () => {
  it('新增空 provider 并切换 active key', () => {
    const id = addAiProvider()
    expect(config.aiProviders).toHaveLength(2)
    expect(config.activeProviderModelKey).toBe(`${id}::`)
  })
})

describe('removeAiProvider', () => {
  it('删激活项时 active key 回退第一项', () => {
    const id2 = addAiProvider()
    config.activeProviderModelKey = `${id2}::`
    removeAiProvider(id2)
    expect(config.aiProviders).toHaveLength(1)
    expect(config.activeProviderModelKey).toBe('p1::')
  })

  it('删空时补默认项维持 ≥1 不变量', () => {
    removeAiProvider('p1')
    expect(config.aiProviders).toHaveLength(1)
    expect(config.aiProviders[0].id).not.toBe('p1')
  })

  it('不存在的 id 无副作用', () => {
    removeAiProvider('not-exist')
    expect(config.aiProviders).toHaveLength(1)
  })
})

describe('updateAiProvider', () => {
  it('部分更新（未传字段保留）', () => {
    updateAiProvider('p1', { endpoint: 'https://api.x.com', apiKey: 'k' })
    expect(config.aiProviders[0].endpoint).toBe('https://api.x.com')
    expect(config.aiProviders[0].apiKey).toBe('k')
    expect(config.aiProviders[0].models).toEqual(['m1'])
  })

  it('不存在的 id 无副作用', () => {
    updateAiProvider('nope', { endpoint: 'x' })
    expect(config.aiProviders[0].endpoint).toBe('')
  })
})
