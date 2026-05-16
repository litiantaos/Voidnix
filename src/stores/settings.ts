import { defineStore } from 'pinia'
import { ref, computed, type Ref } from 'vue'
import { Store, load } from '@tauri-apps/plugin-store'
import { invoke } from '@tauri-apps/api/core'
import { isTauri } from '@/utils/tauri'

export interface ChatApiConfig {
  id: string
  endpoint: string
  apiKey: string
  models: string[]
}

export interface TranslateApiConfig {
  id: string
  type: 'youdao' | 'ai'
  isDefault?: boolean
  // 有道
  appKey: string
  appSecret: string
  // AI
  endpoint: string
  apiKey: string
  models: string[]
  prompt: string
}

function generateId(): string {
  return Date.now().toString(36) + Math.random().toString(36).substr(2, 6)
}

export const useSettingsStore = defineStore('settings', () => {
  let store: Store | null = null

  const globalShortcut = ref('CommandOrControl+Shift+Space')
  const clipboardShortcut = ref('CommandOrControl+Shift+C')
  const clipboardMaxDays = ref(30)
  const translateShortcut = ref('CommandOrControl+Shift+T')
  const chatShortcut = ref('CommandOrControl+Shift+A')
  const screenshotShortcut = ref('CommandOrControl+Shift+X')
  const screenshotSavePath = ref('')  // 空字符串表示使用下载文件夹

  // 翻译设置
  const youdaoAppKey = ref('')
  const youdaoAppSecret = ref('')
  const translateTargetLang = ref('zh')

  // 翻译提供商配置
  const translateConfigs = ref<TranslateApiConfig[]>([
    {
      id: generateId(),
      type: 'youdao',
      isDefault: true,
      appKey: '',
      appSecret: '',
      endpoint: '',
      apiKey: '',
      models: [],
      prompt: '',
    },
  ])

  // 格式：configId::modelName，用于 AI 类型的翻译提供商
  const activeTranslateModelKey = ref('')

  // 从 activeTranslateModelKey 解析当前 AI 翻译 config
  const activeTranslateConfig = computed<TranslateApiConfig | null>(() => {
    const key = activeTranslateModelKey.value
    const sep = key.indexOf('::')
    if (sep !== -1) {
      const id = key.substring(0, sep)
      const found = translateConfigs.value.find((c) => c.id === id)
      if (found) return found
    }
    // 回退到第一个 AI 配置
    return translateConfigs.value.find((c) => c.type === 'ai') || null
  })

  // AI Chat 设置
  const chatConfigs = ref<ChatApiConfig[]>([
    {
      id: generateId(),
      endpoint: '',
      apiKey: '',
      models: [],
    },
  ])

  // 格式：configId::modelName，统一来源
  const activeModelKey = ref('')

  // 从 activeModelKey 解析当前 config
  const activeChatConfig = computed<ChatApiConfig>(() => {
    const key = activeModelKey.value
    const sep = key.indexOf('::')
    if (sep !== -1) {
      const id = key.substring(0, sep)
      const found = chatConfigs.value.find((c) => c.id === id)
      if (found) return found
    }
    return chatConfigs.value[0]
  })

  // Finder extension
  const finderExtEnabled = ref(false)

  function createSetter<T>(r: Ref<T>, key: string) {
    return async (val: T) => {
      r.value = val
      if (store) {
        await store.set(key, val)
        await store.save()
      }
    }
  }

  async function saveChatConfigs() {
    if (store) {
      await store.set('chatConfigs', chatConfigs.value)
      await store.set('activeModelKey', activeModelKey.value)
      await store.save()
    }
  }

  async function saveTranslateConfigs() {
    if (store) {
      await store.set('translateConfigs', translateConfigs.value)
      await store.set('activeTranslateModelKey', activeTranslateModelKey.value)
      await store.save()
    }
  }

  async function addTranslateConfig(): Promise<string> {
    const config: TranslateApiConfig = {
      id: generateId(),
      type: 'ai',
      appKey: '',
      appSecret: '',
      endpoint: '',
      apiKey: '',
      models: [],
      prompt: '',
    }
    translateConfigs.value.push(config)
    activeTranslateModelKey.value = `${config.id}::`
    await saveTranslateConfigs()
    return config.id
  }

  async function removeTranslateConfig(id: string) {
    const config = translateConfigs.value.find((c) => c.id === id)
    if (!config || config.isDefault) return
    const idx = translateConfigs.value.findIndex((c) => c.id === id)
    if (idx === -1) return
    translateConfigs.value.splice(idx, 1)
    if (activeTranslateModelKey.value.startsWith(`${id}::`)) {
      const first = translateConfigs.value.find((c) => c.type === 'ai')
      if (first) {
        activeTranslateModelKey.value = `${first.id}::${first.models[0] || ''}`
      } else {
        activeTranslateModelKey.value = ''
      }
    }
    await saveTranslateConfigs()
  }

  async function updateTranslateConfig(
    id: string,
    patch: Partial<Omit<TranslateApiConfig, 'id' | 'type'>>,
  ) {
    const config = translateConfigs.value.find((c) => c.id === id)
    if (!config) return
    Object.assign(config, patch)
    await saveTranslateConfigs()
  }

  async function addChatConfig(): Promise<string> {
    const config: ChatApiConfig = {
      id: generateId(),
      endpoint: '',
      apiKey: '',
      models: [],
    }
    chatConfigs.value.push(config)
    activeModelKey.value = `${config.id}::`
    await saveChatConfigs()
    return config.id
  }

  async function removeChatConfig(id: string) {
    const idx = chatConfigs.value.findIndex((c) => c.id === id)
    if (idx === -1) return
    chatConfigs.value.splice(idx, 1)
    if (chatConfigs.value.length === 0) {
      await addChatConfig()
    } else if (activeModelKey.value.startsWith(`${id}::`)) {
      const first = chatConfigs.value[0]
      activeModelKey.value = `${first.id}::${first.models[0] || ''}`
    }
    await saveChatConfigs()
  }

  async function updateChatConfig(
    id: string,
    patch: Partial<Omit<ChatApiConfig, 'id'>>,
  ) {
    const config = chatConfigs.value.find((c) => c.id === id)
    if (!config) return
    Object.assign(config, patch)
    await saveChatConfigs()
  }

  async function loadSettings() {
    try {
      store = await load('settings.json', { autoSave: false, defaults: {} })

      const gs = await store.get<string>('globalShortcut')
      if (gs) globalShortcut.value = gs

      const cs = await store.get<string>('clipboardShortcut')
      if (cs) clipboardShortcut.value = cs

      const maxDays = await store.get<number>('clipboardMaxDays')
      if (maxDays !== null && maxDays !== undefined)
        clipboardMaxDays.value = maxDays

      const ts = await store.get<string>('translateShortcut')
      if (ts) translateShortcut.value = ts

      const chs = await store.get<string>('chatShortcut')
      if (chs) chatShortcut.value = chs

      const scs = await store.get<string>('screenshotShortcut')
      if (scs) screenshotShortcut.value = scs

      const ssp = await store.get<string>('screenshotSavePath')
      if (ssp !== null && ssp !== undefined) screenshotSavePath.value = ssp

      const yak = await store.get<string>('youdaoAppKey')
      if (yak) youdaoAppKey.value = yak

      const yas = await store.get<string>('youdaoAppSecret')
      if (yas) youdaoAppSecret.value = yas

      const ttl = await store.get<string>('translateTargetLang')
      if (ttl) translateTargetLang.value = ttl

      // 翻译提供商配置（优先读取新的 translateConfigs）
      const tConfigs = await store.get<Record<string, unknown>[]>('translateConfigs')
      if (tConfigs && tConfigs.length > 0) {
        translateConfigs.value = tConfigs as unknown as TranslateApiConfig[]
        // 标记第一个有道翻译为默认（不可删除）
        const firstYoudao = translateConfigs.value.find((c) => c.type === 'youdao')
        if (firstYoudao) firstYoudao.isDefault = true
        const tKey = await store.get<string>('activeTranslateModelKey')
        if (tKey) activeTranslateModelKey.value = tKey
      } else {
        // 迁移：将旧的 youdaoAppKey/youdaoAppSecret 合入第一个 Youdao 配置
        const youdaoCfg = translateConfigs.value.find((c) => c.type === 'youdao')
        if (youdaoCfg && (youdaoAppKey.value || youdaoAppSecret.value)) {
          youdaoCfg.appKey = youdaoAppKey.value
          youdaoCfg.appSecret = youdaoAppSecret.value
          await saveTranslateConfigs()
        }
      }

      const configs = await store.get<Record<string, unknown>[]>('chatConfigs')
      if (configs && configs.length > 0) {
        chatConfigs.value = configs.map((c: Record<string, unknown>) => {
          if (typeof c.model === 'string') {
            c.models = [c.model]
            delete c.model
          }
          // 清理旧版本遗留字段
          delete c.name
          delete c.compatibility
          delete c.activeModel
          return c as unknown as ChatApiConfig
        })

        // 迁移：优先读 activeModelKey，否则从旧 activeChatConfigId + activeModel 构造
        let key = await store.get<string>('activeModelKey')
        if (!key) {
          const oldId = await store.get<string>('activeChatConfigId')
          const cfg = oldId
            ? chatConfigs.value.find((c) => c.id === oldId)
            : chatConfigs.value[0]
          // 旧数据 activeModel 已清理，取第一个模型
          key = cfg ? `${cfg.id}::${cfg.models[0] || ''}` : ''
        }
        activeModelKey.value = key
      }

      const fee = await store.get<boolean>('finderExtEnabled')
      if (fee !== null && fee !== undefined) finderExtEnabled.value = fee

      // 同步 finder ext 启用状态到后端
      if (isTauri) {
        invoke('set_finder_ext_enabled', { enabled: finderExtEnabled.value }).catch(() => {})
      }
    } catch (e) {
      console.warn('Failed to load settings.json, using defaults:', e)
      try {
        store = await Store.load('settings.json', {
          autoSave: false,
          defaults: {},
        })
      } catch (innerErr) {
        console.warn('Also failed to load Store:', innerErr)
      }
    }
  }

  const setGlobalShortcut = createSetter(globalShortcut, 'globalShortcut')
  const setClipboardShortcut = createSetter(
    clipboardShortcut,
    'clipboardShortcut',
  )
  const setClipboardMaxDays = createSetter(clipboardMaxDays, 'clipboardMaxDays')
  const setTranslateShortcut = createSetter(translateShortcut, 'translateShortcut')
  const setChatShortcut = createSetter(chatShortcut, 'chatShortcut')
  const setScreenshotShortcut = createSetter(screenshotShortcut, 'screenshotShortcut')
  const setScreenshotSavePath = createSetter(screenshotSavePath, 'screenshotSavePath')
  const setYoudaoAppKey = createSetter(youdaoAppKey, 'youdaoAppKey')
  const setYoudaoAppSecret = createSetter(youdaoAppSecret, 'youdaoAppSecret')
  const setTranslateTargetLang = createSetter(
    translateTargetLang,
    'translateTargetLang',
  )
  const setFinderExtEnabled = async (val: boolean) => {
    finderExtEnabled.value = val
    if (store) {
      await store.set('finderExtEnabled', val)
      await store.save()
    }
    if (isTauri) {
      invoke('set_finder_ext_enabled', { enabled: val }).catch(() => {})
    }
  }
  const setActiveModelKey = createSetter(activeModelKey, 'activeModelKey')

  return {
    globalShortcut,
    clipboardShortcut,
    clipboardMaxDays,
    translateShortcut,
    chatShortcut,
    screenshotShortcut,
    screenshotSavePath,
    youdaoAppKey,
    youdaoAppSecret,
    translateTargetLang,
    translateConfigs,
    activeTranslateModelKey,
    activeTranslateConfig,
    chatConfigs,
    activeModelKey,
    activeChatConfig,
    finderExtEnabled,
    loadSettings,
    setGlobalShortcut,
    setClipboardShortcut,
    setClipboardMaxDays,
    setTranslateShortcut,
    setChatShortcut,
    setScreenshotShortcut,
    setScreenshotSavePath,
    setYoudaoAppKey,
    setYoudaoAppSecret,
    setTranslateTargetLang,
    addTranslateConfig,
    removeTranslateConfig,
    updateTranslateConfig,
    setActiveTranslateModelKey: createSetter(activeTranslateModelKey, 'activeTranslateModelKey'),
    addChatConfig,
    removeChatConfig,
    updateChatConfig,
    setActiveModelKey,
    setFinderExtEnabled,
    saveChatConfigs,
    saveTranslateConfigs,
  }
})
