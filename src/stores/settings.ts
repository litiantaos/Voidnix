import { defineStore } from 'pinia'
import { computed } from 'vue'
import { defineConfig } from '@/runtime/storage'

export type Appearance = 'auto' | 'light' | 'dark'
export type Language = 'zh-CN' | 'en'

interface SettingsSchema {
  globalShortcut: string
  shortcutOverrides: Record<string, string>
  /** 外观模式：auto 跟随系统 / light / dark（默认 auto） */
  appearance: Appearance
  /** 语言：zh-CN / en（默认 zh-CN） */
  language: Language
}

/// 框架级配置 store：仅管理全局快捷键。
/// 扩展自管配置一律走 defineConfig（extensions/<id>/config.json）。
/// 本 store 亦走 defineConfig，统一持久化机制（config/settings.json）。
export const useSettingsStore = defineStore('settings', () => {
  const config = defineConfig<SettingsSchema>('config/settings', {
    globalShortcut: 'Alt+Space',
    shortcutOverrides: {},
    appearance: 'auto',
    language: 'zh-CN',
  })

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
  const appearance = computed({
    get: () => config.appearance,
    set: (v: Appearance) => {
      config.appearance = v
    },
  })
  const language = computed({
    get: () => config.language,
    set: (v: Language) => {
      config.language = v
    },
  })

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

  return {
    globalShortcut,
    shortcutOverrides,
    appearance,
    language,
    setGlobalShortcut,
    getShortcutOverride,
    setShortcutOverride,
  }
})
