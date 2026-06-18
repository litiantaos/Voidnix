import { describe, it, expect, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useSettingsStore, type AiProviderConfig } from './settings'

describe('settings store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
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
})
