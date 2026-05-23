import { ref } from 'vue'
import { registerModule } from '@/core/module-registry'
import { asyncView } from '@/core/async-view'
import type { AppModule, SearchResult } from '@/types/module'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useSettingsStore, type TranslateApiConfig } from '@/stores/settings'
import { useAppStore } from '@/stores/app'
import { generateRequestId } from '@/composables/useStreamOutput'
import {
  commands,
  type TranslateResult as BindingsTranslateResult,
} from '@/bindings'

const TranslateView = asyncView(() => import('./View.vue'))
const TranslateSettings = asyncView(() => import('./Settings.vue'))
const TranslateActions = asyncView(() => import('./Actions.vue'))

/** 前端扩展类型：在 bindings 的 TranslateResult 基础上增加 loading 状态 */
export type TranslateResult = BindingsTranslateResult & { loading?: boolean }

export const translateResults = ref<TranslateResult[]>([])
export const isTranslating = ref(false)
export const sourceText = ref('')
export const pendingText = ref('')
export const inputText = ref('')

let unlistenChunk: UnlistenFn | null = null
let unlistenDone: UnlistenFn | null = null
let unlistenReady: UnlistenFn | null = null
let streamInitializing = false

const streamIndexMap = new Map<string, number>()

async function initStreamListeners() {
  if (unlistenChunk || streamInitializing) return
  streamInitializing = true

  try {
    unlistenChunk = await listen<{ requestId: string; content: string }>(
      'translate-chunk',
      (event) => {
        const { requestId, content } = event.payload
        const idx = streamIndexMap.get(requestId)
        if (idx !== undefined && translateResults.value[idx]) {
          translateResults.value[idx].translation += content
        }
      },
    )

    unlistenDone = await listen<{ requestId: string }>(
      'translate-done',
      (event) => {
        const { requestId } = event.payload
        const idx = streamIndexMap.get(requestId)
        if (idx !== undefined && translateResults.value[idx]) {
          translateResults.value[idx].loading = false
        }
        streamIndexMap.delete(requestId)
        checkAllDone()
      },
    )
  } finally {
    streamInitializing = false
  }
}

function checkAllDone() {
  if (translateResults.value.every((r) => !r.loading)) {
    isTranslating.value = false
  }
}

export function destroyStreamListeners() {
  unlistenChunk?.()
  unlistenChunk = null
  unlistenDone?.()
  unlistenDone = null
  unlistenReady?.()
  unlistenReady = null
  streamInitializing = false
}

function engineLabel(config: TranslateApiConfig): string {
  if (config.type === 'youdao') return '有道翻译'
  if (!config.endpoint) return '翻译'
  try {
    const parts = new URL(config.endpoint).hostname.split('.')
    if (parts.length >= 2) return parts[parts.length - 2].toUpperCase()
    return parts[0].toUpperCase()
  } catch {
    return '翻译'
  }
}

export async function translateText(text: string) {
  if (!text.trim()) {
    translateResults.value = []
    return
  }

  sourceText.value = text
  isTranslating.value = true
  streamIndexMap.clear()

  await initStreamListeners()

  const settings = useSettingsStore()
  const configs = settings.translateConfigs
  const targetLang = settings.translateTargetLang
  const promises: Promise<void>[] = []

  const placeholder: TranslateResult[] = []
  let idx = 0

  for (const config of configs) {
    if (config.type === 'youdao') {
      if (!config.appKey || !config.appSecret) continue
      const i = idx++
      const engine = engineLabel(config)
      placeholder.push({ source: text, translation: '', engine, loading: true })
      promises.push(
        commands
          .translateYoudao(text, config.appKey, config.appSecret, targetLang)
          .then((result) => {
            translateResults.value.splice(i, 1, result)
          })
          .catch((e) => {
            const msg = e instanceof Error ? e.message : String(e)
            translateResults.value.splice(i, 1, {
              source: text,
              translation: msg,
              engine,
            })
          })
          .finally(() => checkAllDone()),
      )
    } else if (config.type === 'ai') {
      if (!config.endpoint || !config.apiKey) continue
      const activeModels = config.models.filter((m) => m.trim())
      for (const model of activeModels) {
        const engineSuffix = ` · ${model.trim()}`
        const i = idx++
        const engine = engineLabel(config) + engineSuffix
        placeholder.push({
          source: text,
          translation: '',
          engine,
          loading: true,
        })
        const requestId = generateRequestId()
        streamIndexMap.set(requestId, i)
        promises.push(
          invoke<void>('translate_ai_stream', {
            text,
            endpoint: config.endpoint,
            apiKey: config.apiKey,
            model,
            targetLang,
            prompt: config.prompt ?? null,
            requestId,
          })
            .catch((e: unknown) => {
              const msg = e instanceof Error ? e.message : String(e)
              translateResults.value.splice(i, 1, {
                source: text,
                translation: msg,
                engine,
              })
              streamIndexMap.delete(requestId)
            })
            .finally(() => checkAllDone()),
        )
      }
    }
  }

  translateResults.value = placeholder

  await Promise.all(promises)
}

export async function getSelectedText(): Promise<string> {
  try {
    const text = await commands.getSelectedText()
    return text
  } catch (e) {
    console.error('Failed to get selected text:', e)
    return ''
  }
}

let translateReadyResolver: ((text: string) => void) | null = null

async function waitForSelectedText(): Promise<string> {
  if (translateReadyResolver) {
    translateReadyResolver('')
    translateReadyResolver = null
  }

  return new Promise<string>((resolve) => {
    translateReadyResolver = resolve
    setTimeout(async () => {
      if (translateReadyResolver !== resolve) return
      translateReadyResolver = null
      try {
        const cached = await invoke<string>('get_selected_text_cached')
        if (cached.trim()) {
          resolve(cached)
          return
        }
        const fallback = await invoke<string>('get_selected_text')
        resolve(fallback || '')
      } catch {
        resolve('')
      }
    }, 1500)
  })
}

const mod: AppModule = {
  id: 'translate',
  name: '翻译',
  description: '选词翻译扩展',
  icon: 'i-ri-translate-2',
  keywords: ['translate', '翻译', '翻譯', 'fanyi', 'youdao', '有道'],
  order: 8,
  disableSearchInput: true,
  layout: { view: TranslateView, searchBarAccessory: TranslateActions },
  panel: TranslateSettings,
  onInit: async () => {
    unlistenReady = await listen<string>('translate-text-ready', (e) => {
      if (translateReadyResolver) {
        translateReadyResolver(e.payload || '')
        translateReadyResolver = null
      }
    })
    await initStreamListeners()
  },
  globalShortcuts: [
    {
      id: 'translate',
      default: 'CommandOrControl+Shift+T',
      onExecute: async (wasVisible: boolean) => {
        const appStore = useAppStore()
        if (wasVisible && appStore.activeModuleId === 'translate') {
          invoke('hide_window').catch(() => {})
          return
        }
        appStore.setActiveModule('translate')
        appStore.setSearchQuery('')
        if (wasVisible) {
          return
        }
        try {
          const text = await waitForSelectedText()
          pendingText.value = text.trim()
        } catch {
          pendingText.value = ''
        }
      },
    },
  ],
  onSearch: async (query) => {
    if (!query.trim()) return []
    if (
      'translate'.includes(query.toLowerCase()) ||
      '翻译'.includes(query) ||
      '翻譯'.includes(query)
    ) {
      return [
        {
          id: 'translate-module',
          title: '翻译',
          description: '打开翻译扩展',
          module: 'translate',
          icon: 'i-ri-translate-2',
          score: 100,
          data: { kind: 'module', moduleId: 'translate' },
        },
      ]
    }
    return []
  },
  onModuleSearch: async (query) => {
    if (!query.trim()) return []

    const settings = useSettingsStore()
    const configs = settings.translateConfigs
    const targetLang = settings.translateTargetLang
    const results: SearchResult[] = []
    const promises: Promise<void>[] = []

    for (const config of configs) {
      if (config.type === 'youdao') {
        if (!config.appKey || !config.appSecret) continue
        promises.push(
          commands
            .translateYoudao(query, config.appKey, config.appSecret, targetLang)
            .then((result) => {
              results.push({
                id: `youdao-${Date.now()}`,
                title: result.translation,
                description: `有道翻译 • ${result.source}`,
                module: 'translate',
                icon: 'i-ri-translate-2',
                score: 100,
                data: { isHighlight: true, translation: result.translation },
              })
            })
            .catch((e) => {
              const msg = e instanceof Error ? e.message : String(e)
              results.push({
                id: `youdao-error-${Date.now()}`,
                title: msg,
                description: '有道翻译',
                module: 'translate',
                icon: 'i-ri-error-warning-line',
                score: 100,
              })
            }),
        )
      } else if (config.type === 'ai') {
        if (!config.endpoint || !config.apiKey) continue
        const activeModels = config.models.filter((m) => m.trim())
        for (const model of activeModels) {
          promises.push(
            commands
              .translateAi(
                query,
                config.endpoint,
                config.apiKey,
                model,
                targetLang,
                config.prompt ?? null,
              )
              .then((result) => {
                const label =
                  activeModels.length > 1
                    ? `${result.engine} · ${model.trim()}`
                    : result.engine
                results.push({
                  id: `ai-${Date.now()}`,
                  title: result.translation,
                  description: `${label} • ${result.source}`,
                  module: 'translate',
                  icon: 'i-ri-translate-2',
                  score: 100,
                  data: { isHighlight: true, translation: result.translation },
                })
              })
              .catch((e) => {
                const msg = e instanceof Error ? e.message : String(e)
                results.push({
                  id: `ai-error-${Date.now()}`,
                  title: msg,
                  description:
                    activeModels.length > 1 ? `翻译 · ${model.trim()}` : '翻译',
                  module: 'translate',
                  icon: 'i-ri-error-warning-line',
                  score: 100,
                })
              }),
          )
        }
      }
    }

    await Promise.all(promises)

    if (results.length === 0) {
      results.push({
        id: 'no-config',
        title: '请先配置翻译 API',
        description: '在设置中配置有道翻译或翻译服务',
        module: 'translate',
        icon: 'i-ri-settings-3-line',
        score: 0,
      })
    }

    return results
  },
  onExecute: async (result) => {
    if (result.data?.translation) {
      try {
        await writeText(result.data.translation as string)
        getCurrentWindow().hide()
      } catch (e) {
        console.error('Failed to copy translation:', e)
      }
    }
  },
}

registerModule(mod)