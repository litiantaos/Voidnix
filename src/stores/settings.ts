import { defineStore } from 'pinia'
import { ref, computed, type Ref } from 'vue'
import { Store, load } from '@tauri-apps/plugin-store'
import { invoke } from '@tauri-apps/api/core'
import { isTauri } from '@/utils/tauri'
import { generateRequestId } from '@/composables/useStreamOutput'

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
  appKey: string
  appSecret: string
  endpoint: string
  apiKey: string
  models: string[]
  prompt: string
}

const generateId = generateRequestId

export const useSettingsStore = defineStore('settings', () => {
  let store: Store | null = null

  function parseActiveConfig<T>(
    key: string,
    configs: T[],
    matchFallback?: (configs: T[]) => T | undefined,
  ): T | undefined {
    const sep = key.indexOf('::')
    if (sep !== -1) {
      const id = key.substring(0, sep)
      const found = (configs as Array<{ id: string } & T>).find((c) => c.id === id)
      if (found) return found
    }
    return matchFallback?.(configs)
  }

  const globalShortcut = ref('CommandOrControl+Shift+Space')
  const clipboardMaxDays = ref(30)
  const screenshotSavePath = ref('')

  // 通用快捷键覆盖表：moduleId/.id → 用户自定义快捷键
  const shortcutOverrides = ref<Record<string, string>>({})

  const youdaoAppKey = ref('')
  const youdaoAppSecret = ref('')
  const translateTargetLang = ref('zh')

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
  const activeTranslateConfig = computed<TranslateApiConfig | null>(
    () =>
      parseActiveConfig(activeTranslateModelKey.value, translateConfigs.value, (c) =>
        c.find((c) => c.type === 'ai'),
      ) ?? null,
  )

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
  const activeChatConfig = computed<ChatApiConfig>(
    () => parseActiveConfig(activeModelKey.value, chatConfigs.value, (c) => c[0])!,
  )

  const finderExtEnabled = ref(false)

  const zshAutosuggestionsEnabled = ref(false)

  // 通用模块配置存储（键为 moduleId，值为任意可序列化数据）
  const moduleConfigs = ref<Record<string, unknown>>({})

  const awakeMirrorMode = ref(true)

  function createSetter<T>(r: Ref<T>, key: string) {
    return async (val: T) => {
      r.value = val
      if (store) {
        await store.set(key, val)
        await store.save()
      }
    }
  }

  function createConfigManager<T extends { id: string }>(opts: {
    configs: Ref<T[]>
    activeKey: Ref<string>
    storeKey: string
    activeKeyStoreKey: string
    generateId: () => string
  }) {
    async function save() {
      if (store) {
        await store.set(opts.storeKey, opts.configs.value)
        await store.set(opts.activeKeyStoreKey, opts.activeKey.value)
        await store.save()
      }
    }
    async function add(): Promise<string> {
      const id = opts.generateId()
      opts.configs.value.push({ id } as T)
      opts.activeKey.value = `${id}::`
      await save()
      return id
    }
    async function remove(id: string) {
      const idx = opts.configs.value.findIndex((c) => c.id === id)
      if (idx === -1) return
      opts.configs.value.splice(idx, 1)
      if (opts.activeKey.value.startsWith(`${id}::`)) {
        opts.activeKey.value = opts.configs.value.length > 0 ? `${opts.configs.value[0].id}::` : ''
      }
      await save()
    }
    async function update(id: string, partial: Partial<T>) {
      const config = opts.configs.value.find((c) => c.id === id)
      if (!config) return
      Object.assign(config, partial)
      await save()
    }
    return { save, add, remove, update }
  }

  const chatConfigManager = createConfigManager({
    configs: chatConfigs,
    activeKey: activeModelKey,
    storeKey: 'chatConfigs',
    activeKeyStoreKey: 'activeModelKey',
    generateId: generateRequestId,
  })

  const translateConfigManager = createConfigManager({
    configs: translateConfigs,
    activeKey: activeTranslateModelKey,
    storeKey: 'translateConfigs',
    activeKeyStoreKey: 'activeTranslateModelKey',
    generateId: generateRequestId,
  })

  async function loadSetting<T>(key: string, ref: Ref<T>) {
    const val = await store!.get<T>(key)
    if (val !== null && val !== undefined) {
      ref.value = val
    }
  }

  async function loadSettings() {
    try {
      store = await load('settings.json', { autoSave: false, defaults: {} })

      await loadSetting('globalShortcut', globalShortcut)
      await loadSetting('clipboardMaxDays', clipboardMaxDays)

      // 迁移旧快捷键字段到新的通用覆盖表
      const overrides = await store.get<Record<string, string>>('shortcutOverrides')
      const migrated = { ...(overrides || {}) }

      // 逐个迁移旧键（新表优先，旧键仅当新表无对应记录时回填）
      const oldShortcutMigrations: [string, string][] = [
        ['clipboardShortcut', 'clipboard'],
        ['translateShortcut', 'translate'],
        ['chatShortcut', 'chat'],
        ['screenshotShortcut', 'screenshot'],
      ]
      for (const [oldKey, newId] of oldShortcutMigrations) {
        if (!migrated[newId]) {
          const oldVal = await store.get<string>(oldKey)
          if (oldVal) migrated[newId] = oldVal
        }
      }
      shortcutOverrides.value = migrated

      await loadSetting('screenshotSavePath', screenshotSavePath)
      await loadSetting('youdaoAppKey', youdaoAppKey)
      await loadSetting('youdaoAppSecret', youdaoAppSecret)
      await loadSetting('translateTargetLang', translateTargetLang)

      // 翻译提供商配置（优先读取新的 translateConfigs）
      const tConfigs = await store.get<Record<string, unknown>[]>('translateConfigs')
      if (tConfigs && tConfigs.length > 0) {
        translateConfigs.value = tConfigs as unknown as TranslateApiConfig[]
        // 标记第一个有道翻译为默认（不可删除）
        const firstYoudao = translateConfigs.value.find((c) => c.type === 'youdao')
        if (firstYoudao) firstYoudao.isDefault = true
        await loadSetting('activeTranslateModelKey', activeTranslateModelKey)
      } else {
        // 迁移：将旧的 youdaoAppKey/youdaoAppSecret 合入第一个 Youdao 配置
        const youdaoCfg = translateConfigs.value.find((c) => c.type === 'youdao')
        if (youdaoCfg && (youdaoAppKey.value || youdaoAppSecret.value)) {
          youdaoCfg.appKey = youdaoAppKey.value
          youdaoCfg.appSecret = youdaoAppSecret.value
          await translateConfigManager.save()
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
          const cfg = oldId ? chatConfigs.value.find((c) => c.id === oldId) : chatConfigs.value[0]
          // 旧数据 activeModel 已清理，取第一个模型
          key = cfg ? `${cfg.id}::${cfg.models[0] || ''}` : ''
        }
        activeModelKey.value = key
      }

      await loadSetting('finderExtEnabled', finderExtEnabled)
      await loadSetting('zshAutosuggestionsEnabled', zshAutosuggestionsEnabled)
      await loadSetting('moduleConfigs', moduleConfigs)
      await loadSetting('awakeMirrorMode', awakeMirrorMode)

      // 同步 finder ext 启用状态到后端
      if (isTauri) {
        invoke('set_finder_ext_enabled', { enabled: finderExtEnabled.value }).catch(() => {})
        invoke('set_zsh_autosuggestions_enabled', {
          enabled: zshAutosuggestionsEnabled.value,
        }).catch(() => {})
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
  const setClipboardMaxDays = createSetter(clipboardMaxDays, 'clipboardMaxDays')
  const setScreenshotSavePath = createSetter(screenshotSavePath, 'screenshotSavePath')

  function getShortcutOverride(id: string): string | undefined {
    return shortcutOverrides.value[id]
  }

  async function setShortcutOverride(id: string, value: string) {
    shortcutOverrides.value = { ...shortcutOverrides.value, [id]: value }
    if (store) {
      await store.set('shortcutOverrides', shortcutOverrides.value)
      await store.save()
    }
  }

  function getModuleConfig<T>(moduleId: string): T | undefined {
    return moduleConfigs.value[moduleId] as T | undefined
  }

  async function setModuleConfig(moduleId: string, config: unknown) {
    moduleConfigs.value = { ...moduleConfigs.value, [moduleId]: config }
    if (store) {
      await store.set('moduleConfigs', moduleConfigs.value)
      await store.save()
    }
  }
  const setYoudaoAppKey = createSetter(youdaoAppKey, 'youdaoAppKey')
  const setYoudaoAppSecret = createSetter(youdaoAppSecret, 'youdaoAppSecret')
  const setTranslateTargetLang = createSetter(translateTargetLang, 'translateTargetLang')
  function createSyncedSetter(ref: Ref<boolean>, storeKey: string, tauriCommand?: string) {
    return async (val: boolean) => {
      ref.value = val
      if (store) {
        await store.set(storeKey, val)
        await store.save()
      }
      if (tauriCommand && isTauri) {
        invoke(tauriCommand, { enabled: val }).catch(() => {})
      }
    }
  }

  const setFinderExtEnabled = createSyncedSetter(
    finderExtEnabled,
    'finderExtEnabled',
    'set_finder_ext_enabled',
  )
  const setZshAutosuggestionsEnabled = createSyncedSetter(
    zshAutosuggestionsEnabled,
    'zshAutosuggestionsEnabled',
    'set_zsh_autosuggestions_enabled',
  )
  const setActiveModelKey = createSetter(activeModelKey, 'activeModelKey')
  const setAwakeMirrorMode = createSetter(awakeMirrorMode, 'awakeMirrorMode')

  return {
    globalShortcut,
    clipboardMaxDays,
    screenshotSavePath,
    shortcutOverrides,
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
    awakeMirrorMode,
    zshAutosuggestionsEnabled,
    loadSettings,
    setGlobalShortcut,
    setClipboardMaxDays,
    setScreenshotSavePath,
    getShortcutOverride,
    setShortcutOverride,
    getModuleConfig,
    setModuleConfig,
    setYoudaoAppKey,
    setYoudaoAppSecret,
    setTranslateTargetLang,
    addChatConfig: chatConfigManager.add,
    removeChatConfig: chatConfigManager.remove,
    updateChatConfig: chatConfigManager.update,
    setActiveTranslateModelKey: createSetter(activeTranslateModelKey, 'activeTranslateModelKey'),
    addTranslateConfig: translateConfigManager.add,
    removeTranslateConfig: translateConfigManager.remove,
    updateTranslateConfig: translateConfigManager.update,
    setActiveModelKey,
    setFinderExtEnabled,
    setAwakeMirrorMode,
    setZshAutosuggestionsEnabled,
    saveChatConfigs: chatConfigManager.save,
    saveTranslateConfigs: translateConfigManager.save,
  }
})
