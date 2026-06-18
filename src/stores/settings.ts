import { defineStore } from 'pinia'
import { ref, computed, type Ref } from 'vue'
import { Store, load } from '@tauri-apps/plugin-store'
import { invoke } from '@tauri-apps/api/core'
import { commands } from '@/bindings'
import { isTauri } from '@/utils/tauri'
import { generateRequestId } from '@/utils/id'

export interface AiProviderConfig {
  id: string
  endpoint: string
  apiKey: string
  models: string[]
}

/// 搜索提供商配置（与 AiProviderConfig 同款多 provider 体系）。
/// Phase 1 仅支持 Tavily；保留 type 字段为未来扩展 Brave/Serper 等留接口。
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

  const globalShortcut = ref('CommandOrControl+Shift+Space')
  const clipboardMaxDays = ref(30)
  const screenshotSavePath = ref('')

  const shortcutOverrides = ref<Record<string, string>>({})

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

  const activeTranslateModelKey = ref('')

  const aiProviders = ref<AiProviderConfig[]>([
    {
      id: generateId(),
      endpoint: '',
      apiKey: '',
      models: [],
    },
  ])

  const activeProviderModelKey = ref('')

  const activeProviderConfig = computed<AiProviderConfig>(
    () => parseActiveConfig(activeProviderModelKey.value, aiProviders.value, (c) => c[0])!,
  )

  // ─── Agent 扩展配置 ────────────────────────────────────────
  // 工具调用默认启用（无开关），仅配置搜索提供商 + 受信命令白名单
  const searchProviders = ref<SearchProviderConfig[]>([
    { id: generateId(), type: 'tavily', apiKey: '' },
  ])
  const activeSearchProviderId = ref('')
  const activeSearchProvider = computed<SearchProviderConfig>(
    () =>
      searchProviders.value.find((p) => p.id === activeSearchProviderId.value) ||
      searchProviders.value[0],
  )
  const agentTrustedCommands = ref<string[]>([
    // 默认白名单（读 + 编辑常用命令），用户可在 settings 自由编辑
    'ls',
    'cat',
    'pwd',
    'echo',
    'head',
    'tail',
    'wc',
    'file',
    'stat',
    'date',
    'which',
    'whoami',
    'uname',
    'find',
    'grep',
    'rg',
    'fd',
    'ag',
    'tree',
    'diff',
    'comm',
    'cmp',
    'md5sum',
    'shasum',
    'mkdir',
    'touch',
    'cp',
    'mv',
    'ln',
    'tee',
    'truncate',
    'sed',
    'awk',
    'sort',
    'uniq',
    'cut',
    'tr',
    'paste',
    'expand',
    'jq',
    'yq',
    'bat',
    'git',
  ])
  const agentSystemPrompt = ref('')

  const finderExtEnabled = ref(false)

  const zshAutosuggestionsEnabled = ref(false)

  const awakeMirrorMode = ref(true)

  const wmCustomWidth = ref(1200)
  const wmCustomHeight = ref(800)
  const wmDragSnapEnabled = ref(true)

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

  async function loadSettings() {
    try {
      store = await load('config/settings.json', { autoSave: false, defaults: {} })

      const shortcuts = await store.get<{ global?: string; overrides?: Record<string, string> }>(
        'shortcuts',
      )
      if (shortcuts?.global) globalShortcut.value = shortcuts.global
      if (shortcuts?.overrides) shortcutOverrides.value = shortcuts.overrides

      const clipboard = await store.get<{ maxDays?: number }>('clipboard')
      if (clipboard?.maxDays != null) clipboardMaxDays.value = clipboard.maxDays

      const screenshot = await store.get<{ savePath?: string }>('screenshot')
      if (screenshot?.savePath != null) screenshotSavePath.value = screenshot.savePath

      const translate = await store.get<{
        targetLang?: string
        configs?: TranslateApiConfig[]
        activeProviderModelKey?: string
      }>('translate')
      if (translate?.targetLang) translateTargetLang.value = translate.targetLang
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

      // agent 扩展自管配置（顶层 'agent' 分组）
      const agent = await store.get<{
        searchProviders?: SearchProviderConfig[]
        activeSearchProviderId?: string
        trustedCommands?: string[]
        systemPrompt?: string
      }>('agent')
      if (agent?.searchProviders?.length) {
        searchProviders.value = agent.searchProviders
      }
      if (agent?.activeSearchProviderId) {
        activeSearchProviderId.value = agent.activeSearchProviderId
      }
      if (agent?.trustedCommands != null) agentTrustedCommands.value = agent.trustedCommands
      if (agent?.systemPrompt != null) agentSystemPrompt.value = agent.systemPrompt

      const extensions = await store.get<{
        finderExt?: boolean
        zshAutosuggestions?: boolean
        awakeMirrorMode?: boolean
        windowManager?: { customWidth?: number; customHeight?: number; dragSnapEnabled?: boolean }
      }>('extensions')
      if (extensions?.finderExt != null) finderExtEnabled.value = extensions.finderExt
      if (extensions?.zshAutosuggestions != null)
        zshAutosuggestionsEnabled.value = extensions.zshAutosuggestions
      if (extensions?.awakeMirrorMode != null) awakeMirrorMode.value = extensions.awakeMirrorMode
      if (extensions?.windowManager?.customWidth != null)
        wmCustomWidth.value = extensions.windowManager.customWidth
      if (extensions?.windowManager?.customHeight != null)
        wmCustomHeight.value = extensions.windowManager.customHeight
      if (extensions?.windowManager?.dragSnapEnabled != null)
        wmDragSnapEnabled.value = extensions.windowManager.dragSnapEnabled

      if (isTauri) {
        invoke('set_finder_ext_enabled', { enabled: finderExtEnabled.value }).catch(() => {})
        invoke('set_zsh_autosuggestions_enabled', {
          enabled: zshAutosuggestionsEnabled.value,
        }).catch(() => {})
        commands
          .toggleDragSnap(wmDragSnapEnabled.value, wmCustomWidth.value, wmCustomHeight.value)
          .catch(() => {})
      }
    } catch (e) {
      console.warn('Failed to load config/settings.json, using defaults:', e)
      try {
        store = await Store.load('config/settings.json', {
          autoSave: false,
          defaults: {},
        })
      } catch (innerErr) {
        console.warn('Also failed to load Store:', innerErr)
      }
    }
  }

  const setGlobalShortcut = createSetter(globalShortcut, 'shortcuts', 'global')
  const setClipboardMaxDays = createSetter(clipboardMaxDays, 'clipboard', 'maxDays')
  const setScreenshotSavePath = createSetter(screenshotSavePath, 'screenshot', 'savePath')

  function getShortcutOverride(id: string): string | undefined {
    return shortcutOverrides.value[id]
  }

  async function setShortcutOverride(id: string, value: string) {
    shortcutOverrides.value = { ...shortcutOverrides.value, [id]: value }
    if (store) {
      await store.set('shortcuts', {
        global: globalShortcut.value,
        overrides: shortcutOverrides.value,
      })
      await store.save()
    }
  }

  const setTranslateTargetLang = createSetter(translateTargetLang, 'translate', 'targetLang')

  function createSyncedSetter(
    r: Ref<boolean>,
    groupKey: string,
    field: string,
    tauriCommand?: string,
  ) {
    return async (val: boolean) => {
      const oldVal = r.value
      r.value = val
      if (store) {
        const group = (await store.get<Record<string, unknown>>(groupKey)) || {}
        group[field] = val
        await store.set(groupKey, group)
        await store.save()
      }
      if (tauriCommand && isTauri) {
        try {
          await invoke(tauriCommand, { enabled: val })
        } catch (e) {
          // revert ref + store，保留原始错误向上抛
          r.value = oldVal
          if (store) {
            try {
              const group = (await store.get<Record<string, unknown>>(groupKey)) || {}
              group[field] = oldVal
              await store.set(groupKey, group)
              await store.save()
            } catch {
              /* revert 失败忽略，不掩盖原始错误 */
            }
          }
          throw e
        }
      }
    }
  }

  const setFinderExtEnabled = createSyncedSetter(
    finderExtEnabled,
    'extensions',
    'finderExt',
    'set_finder_ext_enabled',
  )

  const setZshAutosuggestionsEnabled = createSyncedSetter(
    zshAutosuggestionsEnabled,
    'extensions',
    'zshAutosuggestions',
    'set_zsh_autosuggestions_enabled',
  )

  const setActiveProviderModelKey = createSetter(
    activeProviderModelKey,
    'aiProviders',
    'activeProviderModelKey',
  )

  // ─── Agent 扩展配置 setters ───
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
    // 不允许删除最后一个 provider
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

  async function setAgentTrustedCommands(val: string[]) {
    agentTrustedCommands.value = val
    if (store) {
      const group = (await store.get<Record<string, unknown>>('agent')) || {}
      group.trustedCommands = val
      await store.set('agent', group)
      await store.save()
    }
  }

  async function setAgentSystemPrompt(val: string) {
    agentSystemPrompt.value = val
    if (store) {
      const group = (await store.get<Record<string, unknown>>('agent')) || {}
      group.systemPrompt = val
      await store.set('agent', group)
      await store.save()
    }
  }

  /// 把「执行并信任」的命令加入持久化白名单（去重）
  async function trustCommand(cmd: string) {
    const name = cmd.trim()
    if (!name) return
    if (agentTrustedCommands.value.includes(name)) return
    agentTrustedCommands.value = [...agentTrustedCommands.value, name]
    await setAgentTrustedCommands(agentTrustedCommands.value)
  }
  const setAwakeMirrorMode = createSetter(awakeMirrorMode, 'extensions', 'awakeMirrorMode')

  async function setWmField(field: string, val: number | boolean) {
    if (store) {
      const group = (await store.get<Record<string, unknown>>('extensions')) || {}
      const wm = (group.windowManager as Record<string, unknown>) || {}
      wm[field] = val
      group.windowManager = wm
      await store.set('extensions', group)
      await store.save()
    }
  }

  function refreshDragSnap() {
    if (isTauri && wmDragSnapEnabled.value) {
      commands.toggleDragSnap(true, wmCustomWidth.value, wmCustomHeight.value).catch(() => {})
    }
  }

  async function setWmCustomWidth(val: number) {
    wmCustomWidth.value = val
    await setWmField('customWidth', val)
    refreshDragSnap()
  }
  async function setWmCustomHeight(val: number) {
    wmCustomHeight.value = val
    await setWmField('customHeight', val)
    refreshDragSnap()
  }
  async function setWmDragSnapEnabled(val: boolean) {
    wmDragSnapEnabled.value = val
    await setWmField('dragSnapEnabled', val)
    if (isTauri) {
      commands.toggleDragSnap(val, wmCustomWidth.value, wmCustomHeight.value).catch(() => {})
    }
  }

  return {
    globalShortcut,
    clipboardMaxDays,
    screenshotSavePath,
    shortcutOverrides,
    translateTargetLang,
    translateConfigs,
    aiProviders,
    activeProviderModelKey,
    activeProviderConfig,
    searchProviders,
    activeSearchProviderId,
    activeSearchProvider,
    agentTrustedCommands,
    agentSystemPrompt,
    finderExtEnabled,
    awakeMirrorMode,
    zshAutosuggestionsEnabled,
    wmCustomWidth,
    wmCustomHeight,
    wmDragSnapEnabled,
    loadSettings,
    setGlobalShortcut,
    setClipboardMaxDays,
    setScreenshotSavePath,
    getShortcutOverride,
    setShortcutOverride,
    setTranslateTargetLang,
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
    setAgentTrustedCommands,
    setAgentSystemPrompt,
    trustCommand,
    setFinderExtEnabled,
    setAwakeMirrorMode,
    setZshAutosuggestionsEnabled,
    setWmCustomWidth,
    setWmCustomHeight,
    setWmDragSnapEnabled,
  }
})
