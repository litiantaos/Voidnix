import { defineStore } from 'pinia'
import { computed } from 'vue'
import { defineConfig } from '@/runtime/storage'
import { generateRequestId } from '@/utils/id'

export interface AiProviderConfig {
  id: string
  endpoint: string
  apiKey: string
  models: string[]
}

const generateId = generateRequestId

interface SettingsSchema {
  globalShortcut: string
  shortcutOverrides: Record<string, string>
  aiProviders: AiProviderConfig[]
  activeProviderModelKey: string
}

/// 框架级配置 store：仅管理全局快捷键 + AI Provider 基础设施。
/// 扩展自管配置一律走 defineConfig（extensions/<id>/config.json）。
/// 本 store 亦走 defineConfig，统一持久化机制（config/settings.json）。
export const useSettingsStore = defineStore('settings', () => {
  const config = defineConfig<SettingsSchema>('config/settings', {
    globalShortcut: 'CommandOrControl+Shift+Space',
    shortcutOverrides: {},
    // 不变量：aiProviders 始终 ≥1 项（removeAiProvider 删空时补默认项），
    // activeProviderConfig 的非空断言依赖此不变量。
    aiProviders: [{ id: generateId(), endpoint: '', apiKey: '', models: [] }],
    activeProviderModelKey: '',
  })

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

  // ─── 字段（可写 computed：保持 store API 兼容） ───────────────

  const globalShortcut = computed({
    get: () => config.globalShortcut,
    set: (v: string) => {
      config.globalShortcut = v
    },
  })
  const shortcutOverrides = computed({
    get: () => config.shortcutOverrides,
    set: (v: Record<string, string>) => {
      config.shortcutOverrides = v
    },
  })
  const aiProviders = computed({
    get: () => config.aiProviders,
    set: (v: AiProviderConfig[]) => {
      config.aiProviders = v
    },
  })
  const activeProviderModelKey = computed({
    get: () => config.activeProviderModelKey,
    set: (v: string) => {
      config.activeProviderModelKey = v
    },
  })

  const activeProviderConfig = computed<AiProviderConfig>(
    () => parseActiveConfig(activeProviderModelKey.value, aiProviders.value, (c) => c[0])!,
  )

  // ─── Setters（直接 mutate reactive config；defineConfig 自动持久化） ────

  async function setGlobalShortcut(val: string) {
    config.globalShortcut = val
  }

  function getShortcutOverride(id: string): string | undefined {
    return config.shortcutOverrides[id]
  }

  async function setShortcutOverride(id: string, value: string) {
    config.shortcutOverrides = { ...config.shortcutOverrides, [id]: value }
  }

  async function setActiveProviderModelKey(key: string) {
    config.activeProviderModelKey = key
  }

  async function addAiProvider(): Promise<string> {
    const id = generateId()
    config.aiProviders.push({ id, endpoint: '', apiKey: '', models: [] })
    config.activeProviderModelKey = `${id}::`
    return id
  }

  async function removeAiProvider(id: string) {
    const idx = config.aiProviders.findIndex((c) => c.id === id)
    if (idx === -1) return
    config.aiProviders.splice(idx, 1)
    // 删空时补默认项，维持「configs ≥1」不变量（activeProviderConfig 非空断言依赖）
    if (config.aiProviders.length === 0) {
      config.aiProviders.push({ id: generateId(), endpoint: '', apiKey: '', models: [] })
    }
    if (config.activeProviderModelKey.startsWith(`${id}::`)) {
      config.activeProviderModelKey = `${config.aiProviders[0].id}::`
    }
  }

  async function updateAiProvider(id: string, partial: Partial<AiProviderConfig>) {
    const target = config.aiProviders.find((c) => c.id === id)
    if (!target) return
    Object.assign(target, partial)
  }

  return {
    globalShortcut,
    shortcutOverrides,
    aiProviders,
    activeProviderModelKey,
    activeProviderConfig,
    setGlobalShortcut,
    getShortcutOverride,
    setShortcutOverride,
    setActiveProviderModelKey,
    addAiProvider,
    removeAiProvider,
    updateAiProvider,
  }
})
