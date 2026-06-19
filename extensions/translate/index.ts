import { config as translateConfig } from './config'
import { ref } from 'vue'
import { defineExtension } from '@/runtime/extension-registry'
import { defineAsyncComponent } from 'vue'
import { makeToggleHandler } from '@/utils/module-toggle'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { toErrorMessage } from '@/utils/format'
import { generateRequestId } from '@/utils/id'
import { cleanStreamResult, engineLabel } from './logic'

const TranslateView = defineAsyncComponent(() => import('./View.vue'))
const TranslateSettings = defineAsyncComponent(() => import('./Settings.vue'))
const TranslateActions = defineAsyncComponent(() => import('./Actions.vue'))

/** Rust translate_ai/translate_youdao 返回结构 */
interface TranslateBaseResult {
  source: string
  translation: string
  engine: string
}

/** 前端扩展类型：在基础结果上增加 loading 状态 */
export type TranslateResult = TranslateBaseResult & { loading?: boolean }

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

    unlistenDone = await listen<{ requestId: string }>('translate-done', (event) => {
      const { requestId } = event.payload
      const idx = streamIndexMap.get(requestId)
      if (idx !== undefined && translateResults.value[idx]) {
        translateResults.value[idx].translation = cleanStreamResult(
          translateResults.value[idx].translation,
        )
        translateResults.value[idx].loading = false
      }
      streamIndexMap.delete(requestId)
      checkAllDone()
    })
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

export async function translateText(text: string) {
  if (!text.trim()) {
    translateResults.value = []
    return
  }

  sourceText.value = text
  isTranslating.value = true
  streamIndexMap.clear()

  await initStreamListeners()

  const configs = translateConfig.configs
  const targetLang = translateConfig.targetLang
  const promises: Promise<void>[] = []

  const placeholder: TranslateResult[] = []
  const youdaoIndexMap = new Map<string, number>()

  for (const config of configs) {
    if (config.type === 'youdao') {
      if (!config.appKey || !config.appSecret) continue
      const requestId = generateRequestId()
      const engine = engineLabel(config)
      const i = placeholder.length
      placeholder.push({ source: text, translation: '', engine, loading: true })
      youdaoIndexMap.set(requestId, i)
      promises.push(
        invoke<TranslateBaseResult>(CMD.translateYoudao, {
          text,
          appKey: config.appKey,
          appSecret: config.appSecret,
          targetLang,
        })
          .then((result) => {
            const idx = youdaoIndexMap.get(requestId)
            if (idx !== undefined && translateResults.value[idx]) {
              translateResults.value.splice(idx, 1, result)
            }
          })
          .catch((e) => {
            const idx = youdaoIndexMap.get(requestId)
            const msg = toErrorMessage(e)
            if (idx !== undefined) {
              translateResults.value.splice(idx, 1, {
                source: text,
                translation: msg,
                engine,
              })
            }
          })
          .finally(() => checkAllDone()),
      )
    } else if (config.type === 'ai') {
      if (!config.endpoint || !config.apiKey) continue
      const activeModels = config.models.filter((m) => m.trim())
      for (const model of activeModels) {
        const engineSuffix = ` · ${model.trim()}`
        const engine = engineLabel(config) + engineSuffix
        const i = placeholder.length
        placeholder.push({
          source: text,
          translation: '',
          engine,
          loading: true,
        })
        const requestId = generateRequestId()
        streamIndexMap.set(requestId, i)
        promises.push(
          invoke<void>(CMD.translateAiStream, {
            text,
            endpoint: config.endpoint,
            apiKey: config.apiKey,
            model,
            targetLang,
            prompt: config.prompt ?? null,
            requestId,
          })
            .catch((e: unknown) => {
              const idx = streamIndexMap.get(requestId)
              const msg = toErrorMessage(e)
              if (idx !== undefined) {
                translateResults.value.splice(idx, 1, {
                  source: text,
                  translation: msg,
                  engine,
                })
              }
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
    const text = await invoke<string>(CMD.getSelectedText)
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
        const cached = await invoke<string>(CMD.getSelectedTextCached)
        if (cached.trim()) {
          resolve(cached)
          return
        }
        const fallback = await invoke<string>(CMD.getSelectedText)
        resolve(fallback || '')
      } catch {
        resolve('')
      }
    }, 1500)
  })
}

export default defineExtension({
  meta: {
    id: 'translate',
    name: '翻译',
    description: '选词翻译',
    icon: 'i-ri-translate-2',
    keywords: ['translate', '翻译', '翻譯', 'fanyi', 'youdao', '有道'],
    order: 8,
  },

  disableSearchInput: true,
  mainView: () => TranslateView,
  searchBarAccessory: () => TranslateActions,
  subviews: { settings: () => TranslateSettings },
  setup: async () => {
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
      onExecute: makeToggleHandler('translate', async () => {
        try {
          const text = await waitForSelectedText()
          pendingText.value = text.trim()
        } catch {
          pendingText.value = ''
        }
      }),
    },
  ],
})
