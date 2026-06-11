import { describe, it, expect, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useSettingsStore, type ChatApiConfig } from './settings'

describe('settings store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
  })

  it('初始默认值', () => {
    const store = useSettingsStore()
    expect(store.globalShortcut).toBe('CommandOrControl+Shift+Space')
    expect(store.clipboardMaxDays).toBe(30)
    expect(store.screenshotSavePath).toBe('')
    expect(store.translateTargetLang).toBe('zh')
    expect(store.finderExtEnabled).toBe(false)
    expect(store.awakeMirrorMode).toBe(true)
    expect(store.wmCustomWidth).toBe(1200)
    expect(store.wmCustomHeight).toBe(800)
    expect(store.wmDragSnapEnabled).toBe(true)
  })

  describe('activeChatConfig', () => {
    it('默认返回第一个配置', () => {
      const store = useSettingsStore()
      const config = store.activeChatConfig
      expect(config).toBeDefined()
      expect(config).toBe(store.chatConfigs[0])
    })

    it('通过 ID:: 前缀匹配指定配置', () => {
      const store = useSettingsStore()
      const targetId = store.chatConfigs[0].id
      store.activeModelKey = `${targetId}::`
      expect(store.activeChatConfig.id).toBe(targetId)
    })

    it('未匹配时回退到第一个', () => {
      const store = useSettingsStore()
      store.activeModelKey = 'nonexistent::'
      expect(store.activeChatConfig).toBe(store.chatConfigs[0])
    })
  })

  describe('shortcutOverride', () => {
    it('getShortcutOverride 返回已设置的值', () => {
      const store = useSettingsStore()
      expect(store.getShortcutOverride('translate')).toBeUndefined()
    })
  })

  describe('chatConfigs 默认结构', () => {
    it('初始包含一个配置', () => {
      const store = useSettingsStore()
      expect(store.chatConfigs).toHaveLength(1)
      const config = store.chatConfigs[0] as ChatApiConfig
      expect(config.id).toBeTruthy()
      expect(config.endpoint).toBe('')
      expect(config.apiKey).toBe('')
      expect(config.models).toEqual([])
    })
  })

  describe('translateConfigs 默认结构', () => {
    it('初始包含一个有道配置', () => {
      const store = useSettingsStore()
      expect(store.translateConfigs).toHaveLength(1)
      expect(store.translateConfigs[0].type).toBe('youdao')
    })
  })
})
