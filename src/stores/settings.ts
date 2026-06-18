import { defineStore } from 'pinia'
import { ref, computed, type Ref } from 'vue'
import { Store, load } from '@tauri-apps/plugin-store'
import { generateRequestId } from '@/utils/id'

export interface AiProviderConfig {
  id: string
  endpoint: string
  apiKey: string
  models: string[]
}

export interface SearchProviderConfig {
  id: string
  type: 'tavily'
  apiKey: string
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

  // ─── 框架级配置 ────────────────────────────────────────────
  const globalShortcut = ref('CommandOrControl+Shift+Space')
  const shortcutOverrides = ref<Record<string, string>>({})

  // ─── AI Provider 基础设施（translate + agent 共享）─────────
  const aiProviders = ref<AiProviderConfig[]>([
    { id: generateId(), endpoint: '', apiKey: '', models: [] },
  ])
  const activeProviderModelKey = ref('')
  const activeProviderConfig = computed<AiProviderConfig>(
    () => parseActiveConfig(activeProviderModelKey.value, aiProviders.value, (c) => c[0])!,
  )

  // ─── translate 扩展配置 ────────────────────────────────────
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
  const activeTranslateModelKey = ref('')

  // ─── agent 扩展配置 ────────────────────────────────────────
  const searchProviders = ref<SearchProviderConfig[]>([
    { id: generateId(), type: 'tavily', apiKey: '' },
  ])
  const activeSearchProviderId = ref('')
  const activeSearchProvider = computed<SearchProviderConfig>(
    () =>
      searchProviders.value.find((p) => p.id === activeSearchProviderId.value) ||
      searchProviders.value[0],
  )

  // ─── 持久化工具 ────────────────────────────────────────────

  function createSetter<T>(r: Ref<T>, groupKey: string, field: string) {
    return async (val: T) => {
      r.value = val
      if (store) {
        const group = (await store.get<Record<string, unknown>>(groupKey)) || {}
        group[field] = val
        await store.set(groupKey, group)
        await store.save()
      }
    }
  }

  function createConfigManager<T extends { id: string }>(opts: {
    configs: Ref<T[]>
    activeKey: Ref<string>
    groupKey: string
    configField: string
    activeField: string
    generateId: () => string
  }) {
    async function save() {
      if (store) {
        const group = (await store.get<Record<string, unknown>>(opts.groupKey)) || {}
        group[opts.configField] = opts.configs.value
        group[opts.activeField] = opts.activeKey.value
        await store.set(opts.groupKey, group)
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

  const aiProviderConfigManager = createConfigManager({
    configs: aiProviders,
    activeKey: activeProviderModelKey,
    groupKey: 'aiProviders',
    configField: 'configs',
    activeField: 'activeProviderModelKey',
    generateId: generateRequestId,
  })

  const translateConfigManager = createConfigManager({
    configs: translateConfigs,
    activeKey: activeTranslateModelKey,
    groupKey: 'translate',
    configField: 'configs',
    activeField: 'activeProviderModelKey',
    generateId: generateRequestId,
  })

  // ─── 加载 ──────────────────────────────────────────────────

  async function loadSettings() {
    try {
      store = await load('config/settings.json', { autoSave: false, defaults: {} })

      const shortcuts = await store.get<{ global?: string; overrides?: Record<string, string> }>(
        'shortcuts',
      )
      if (shortcuts?.global) globalShortcut.value = shortcuts.global
      if (shortcuts?.overrides) shortcutOverrides.value = shortcuts.overrides

      const translate = await store.get<{
        configs?: TranslateApiConfig[]
        activeProviderModelKey?: string
      }>('translate')
      if (translate?.configs?.length) {
        translateConfigs.value = translate.configs
        const firstYoudao = translateConfigs.value.find((c) => c.type === 'youdao')
        if (firstYoudao) firstYoudao.isDefault = true
      }
      if (translate?.activeProviderModelKey)
        activeTranslateModelKey.value = translate.activeProviderModelKey

      const aiProvidersData = await store.get<{
        configs?: AiProviderConfig[]
        activeProviderModelKey?: string
      }>('aiProviders')
      if (aiProvidersData?.configs?.length) aiProviders.value = aiProvidersData.configs
      if (aiProvidersData?.activeProviderModelKey)
        activeProviderModelKey.value = aiProvidersData.activeProviderModelKey

      const agent = await store.get<{
        searchProviders?: SearchProviderConfig[]
        activeSearchProviderId?: string
      }>('agent')
      if (agent?.searchProviders?.length) searchProviders.value = agent.searchProviders
      if (agent?.activeSearchProviderId) activeSearchProviderId.value = agent.activeSearchProviderId
    } catch (e) {
      console.warn('Failed to load config/settings.json, using defaults:', e)
      try {
        store = await Store.load('config/settings.json', { autoSave: false, defaults: {} })
      } catch (innerErr) {
        console.warn('Also failed to load Store:', innerErr)
      }
    }
  }

  // ─── Setters ───────────────────────────────────────────────

  const setGlobalShortcut = createSetter(globalShortcut, 'shortcuts', 'global')

  function getShortcutOverride(id: string): string | undefined {
    return shortcutOverrides.value[id]
  }

  async function setShortcutOverride(id: string, value: string) {
    shortcutOverrides.value = { ...shortcutOverrides.value, [id]: value }
    if (store) {
      await store.set('shortcuts', { global: globalShortcut.value, overrides: shortcutOverrides.value })
      await store.save()
    }
  }

  const setActiveProviderModelKey = createSetter(activeProviderModelKey, 'aiProviders', 'activeProviderModelKey')

  async function saveSearchProviders() {
    if (store) {
      const group = (await store.get<Record<string, unknown>>('agent')) || {}
      group.searchProviders = searchProviders.value
      group.activeSearchProviderId = activeSearchProviderId.value
      await store.set('agent', group)
      await store.save()
    }
  }

  async function addSearchProvider(): Promise<string> {
    const id = generateRequestId()
    searchProviders.value.push({ id, type: 'tavily', apiKey: '' })
    activeSearchProviderId.value = id
    await saveSearchProviders()
    return id
  }

  async function removeSearchProvider(id: string) {
    const idx = searchProviders.value.findIndex((c) => c.id === id)
    if (idx === -1) return
    if (searchProviders.value.length <= 1) return
    searchProviders.value.splice(idx, 1)
    if (activeSearchProviderId.value === id) {
      activeSearchProviderId.value = searchProviders.value[0]?.id || ''
    }
    await saveSearchProviders()
  }

  async function updateSearchProvider(id: string, partial: Partial<SearchProviderConfig>) {
    const config = searchProviders.value.find((c) => c.id === id)
    if (!config) return
    Object.assign(config, partial)
    await saveSearchProviders()
  }

  async function setActiveSearchProviderId(val: string) {
    activeSearchProviderId.value = val
    await saveSearchProviders()
  }

  return {
    globalShortcut,
    shortcutOverrides,
    translateConfigs,
    aiProviders,
    activeProviderModelKey,
    activeProviderConfig,
    searchProviders,
    activeSearchProviderId,
    activeSearchProvider,
    loadSettings,
    setGlobalShortcut,
    getShortcutOverride,
    setShortcutOverride,
    addAiProvider: aiProviderConfigManager.add,
    removeAiProvider: aiProviderConfigManager.remove,
    updateAiProvider: aiProviderConfigManager.update,
    addTranslateConfig: translateConfigManager.add,
    removeTranslateConfig: translateConfigManager.remove,
    updateTranslateConfig: translateConfigManager.update,
    setActiveProviderModelKey,
    addSearchProvider,
    removeSearchProvider,
    updateSearchProvider,
    setActiveSearchProviderId,
  }
})
