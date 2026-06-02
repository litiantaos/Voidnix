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

  const chatConfigs = ref<ChatApiConfig[]>([
    {
      id: generateId(),
      endpoint: '',
      apiKey: '',
      models: [],
    },
  ])

  const activeModelKey = ref('')

  const activeChatConfig = computed<ChatApiConfig>(
    () => parseActiveConfig(activeModelKey.value, chatConfigs.value, (c) => c[0])!,
  )

  const finderExtEnabled = ref(false)

  const zshAutosuggestionsEnabled = ref(false)

  const awakeMirrorMode = ref(true)

  const wmCustomWidth = ref(800)
  const wmCustomHeight = ref(600)
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

  const chatConfigManager = createConfigManager({
    configs: chatConfigs,
    activeKey: activeModelKey,
    groupKey: 'chat',
    configField: 'configs',
    activeField: 'activeModelKey',
    generateId: generateRequestId,
  })

  const translateConfigManager = createConfigManager({
    configs: translateConfigs,
    activeKey: activeTranslateModelKey,
    groupKey: 'translate',
    configField: 'configs',
    activeField: 'activeModelKey',
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
        activeModelKey?: string
      }>('translate')
      if (translate?.targetLang) translateTargetLang.value = translate.targetLang
      if (translate?.configs?.length) {
        translateConfigs.value = translate.configs
        const firstYoudao = translateConfigs.value.find((c) => c.type === 'youdao')
        if (firstYoudao) firstYoudao.isDefault = true
      }

      const chat = await store.get<{
        configs?: ChatApiConfig[]
        activeModelKey?: string
      }>('chat')
      if (chat?.configs?.length) chatConfigs.value = chat.configs
      if (chat?.activeModelKey) activeModelKey.value = chat.activeModelKey

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
        invoke('toggle_drag_snap', {
          enabled: wmDragSnapEnabled.value,
          customWidth: wmCustomWidth.value,
          customHeight: wmCustomHeight.value,
        }).catch(() => {})
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
      r.value = val
      if (store) {
        const group = (await store.get<Record<string, unknown>>(groupKey)) || {}
        group[field] = val
        await store.set(groupKey, group)
        await store.save()
      }
      if (tauriCommand && isTauri) {
        invoke(tauriCommand, { enabled: val }).catch(() => {})
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
  const setActiveModelKey = createSetter(activeModelKey, 'chat', 'activeModelKey')
  const setAwakeMirrorMode = createSetter(awakeMirrorMode, 'extensions', 'awakeMirrorMode')

  async function setWmField(field: string, val: number) {
    if (store) {
      const group = (await store.get<Record<string, unknown>>('extensions')) || {}
      const wm = (group.windowManager as Record<string, unknown>) || {}
      wm[field] = val
      group.windowManager = wm
      await store.set('extensions', group)
      await store.save()
    }
  }
  async function setWmCustomWidth(val: number) {
    wmCustomWidth.value = val
    await setWmField('customWidth', val)
  }
  async function setWmCustomHeight(val: number) {
    wmCustomHeight.value = val
    await setWmField('customHeight', val)
  }
  async function setWmDragSnapEnabled(val: boolean) {
    wmDragSnapEnabled.value = val
    if (store) {
      const group = (await store.get<Record<string, unknown>>('extensions')) || {}
      const wm = (group.windowManager as Record<string, unknown>) || {}
      wm.dragSnapEnabled = val
      group.windowManager = wm
      await store.set('extensions', group)
      await store.save()
    }
    if (isTauri) {
      invoke('toggle_drag_snap', {
        enabled: val,
        customWidth: wmCustomWidth.value,
        customHeight: wmCustomHeight.value,
      }).catch(() => {})
    }
  }

  return {
    globalShortcut,
    clipboardMaxDays,
    screenshotSavePath,
    shortcutOverrides,
    translateTargetLang,
    translateConfigs,
    chatConfigs,
    activeModelKey,
    activeChatConfig,
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
    addChatConfig: chatConfigManager.add,
    removeChatConfig: chatConfigManager.remove,
    updateChatConfig: chatConfigManager.update,
    addTranslateConfig: translateConfigManager.add,
    removeTranslateConfig: translateConfigManager.remove,
    updateTranslateConfig: translateConfigManager.update,
    setActiveModelKey,
    setFinderExtEnabled,
    setAwakeMirrorMode,
    setZshAutosuggestionsEnabled,
    setWmCustomWidth,
    setWmCustomHeight,
    setWmDragSnapEnabled,
  }
})
