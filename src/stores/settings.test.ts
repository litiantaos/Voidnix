import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useSettingsStore, type AiProviderConfig } from './settings'

// 模拟 plugin-store：内存表（get/set/save/clear/onChange）
const memStore = new Map<string, unknown>()
const storeGet = vi.fn((k: string) => Promise.resolve(memStore.get(k)))
const storeSet = vi.fn((k: string, v: unknown) => {
  memStore.set(k, v)
  return Promise.resolve()
})
const storeSave = vi.fn(() => Promise.resolve())
const storeClear = vi.fn(() => {
  memStore.clear()
  return Promise.resolve()
})
const storeOnChange = vi.fn(() => Promise.resolve(() => {}))

vi.mock('@tauri-apps/plugin-store', () => ({
  load: () =>
    Promise.resolve({
      get: storeGet,
      set: storeSet,
      save: storeSave,
      clear: storeClear,
      onChange: storeOnChange,
    }),
}))

vi.mock('@/utils/tauri', () => ({ isTauri: false }))

describe('settings store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    memStore.clear()
    storeGet.mockClear()
    storeSet.mockClear()
    storeSave.mockClear()
    storeClear.mockClear()
  })

  it('初始默认值', () => {
    const store = useSettingsStore()
    expect(store.globalShortcut).toBe('CommandOrControl+Shift+Space')
  })

  describe('activeProviderConfig', () => {
    it('默认返回第一个配置', () => {
      const store = useSettingsStore()
      const config = store.activeProviderConfig
      expect(config).toBeDefined()
      expect(config).toBe(store.aiProviders[0])
    })

    it('通过 ID:: 前缀匹配指定配置', () => {
      const store = useSettingsStore()
      const targetId = store.aiProviders[0].id
      store.activeProviderModelKey = `${targetId}::`
      expect(store.activeProviderConfig.id).toBe(targetId)
    })

    it('未匹配时回退到第一个', () => {
      const store = useSettingsStore()
      store.activeProviderModelKey = 'nonexistent::'
      expect(store.activeProviderConfig).toBe(store.aiProviders[0])
    })
  })

  describe('shortcutOverride', () => {
    it('getShortcutOverride 返回已设置的值', () => {
      const store = useSettingsStore()
      expect(store.getShortcutOverride('translate')).toBeUndefined()
    })

    it('setShortcutOverride 后能读取', async () => {
      const store = useSettingsStore()
      await store.setShortcutOverride('translate', 'CommandOrControl+T')
      expect(store.getShortcutOverride('translate')).toBe('CommandOrControl+T')
    })
  })

  describe('aiProviders 默认结构', () => {
    it('初始包含一个配置', () => {
      const store = useSettingsStore()
      expect(store.aiProviders).toHaveLength(1)
      const config = store.aiProviders[0] as AiProviderConfig
      expect(config.id).toBeTruthy()
      expect(config.endpoint).toBe('')
      expect(config.apiKey).toBe('')
      expect(config.models).toEqual([])
    })
  })

  describe('aiProvider CRUD', () => {
    it('addAiProvider 新增并切换 active key', async () => {
      const store = useSettingsStore()
      const id = await store.addAiProvider()
      expect(store.aiProviders).toHaveLength(2)
      expect(store.activeProviderModelKey).toBe(`${id}::`)
    })

    it('removeAiProvider 删空时补默认项', async () => {
      const store = useSettingsStore()
      const onlyId = store.aiProviders[0].id
      await store.removeAiProvider(onlyId)
      expect(store.aiProviders).toHaveLength(1)
      expect(store.aiProviders[0].id).not.toBe(onlyId)
    })

    it('updateAiProvider 部分更新', async () => {
      const store = useSettingsStore()
      const id = store.aiProviders[0].id
      await store.updateAiProvider(id, { endpoint: 'https://api.x.com', apiKey: 'k' })
      expect(store.aiProviders[0].endpoint).toBe('https://api.x.com')
      expect(store.aiProviders[0].apiKey).toBe('k')
    })
  })
})
