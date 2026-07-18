import { config as translateConfig, resolveAiTargets } from './config'
import { refreshEnvSnapshot } from '@/runtime/ai-providers'
import { ref } from 'vue'
import { defineExtension } from '@/runtime/extension-registry'
import { makeToggleHandler } from '@/stores/app'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { listen } from '@tauri-apps/api/event'
import { toErrorMessage } from '@/utils/format'
import { generateRequestId } from '@/utils/id'
import { cleanStreamResult, engineLabel } from './logic'
import TranslateSettings from './Settings.vue'
import TranslateView from './View.vue'
import TranslateActions from './Actions.vue'

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
export const pendingText = ref('')
export const inputText = ref('')

/** 流式监听应用级常驻（扩展不卸载），只装一次；并发共用同一 Promise。 */
let streamListenersReady = false
let streamInitPromise: Promise<void> | null = null

const streamIndexMap = new Map<string, number>()

async function initStreamListeners() {
  if (streamListenersReady) return
  if (!streamInitPromise) {
    streamInitPromise = doInitStreamListeners().catch((e) => {
      // 允许半失败后重试；成功后 Promise 常驻，不再清空
      streamInitPromise = null
      throw e
    })
  }
  return streamInitPromise
}

async function doInitStreamListeners() {
  // 半失败时 unlisten 已装监听，避免重试双注册 chunk
  let unlistenChunk: (() => void) | null = null
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

    await listen<{ requestId: string }>('translate-done', (event) => {
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
    streamListenersReady = true
    // 成功后不 unlisten：应用级常驻
  } catch (e) {
    unlistenChunk?.()
    throw e
  }
}

function checkAllDone() {
  if (translateResults.value.every((r) => !r.loading)) {
    isTranslating.value = false
  }
}

export async function translateText(text: string) {
  if (!text.trim()) {
    translateResults.value = []
    return
  }

  isTranslating.value = true
  streamIndexMap.clear()

  // 配置缺项时用 env / ai.env 补齐
  await refreshEnvSnapshot()
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
      const targets = resolveAiTargets(config)
      for (const t of targets) {
        const engine = `${t.label} · ${t.model}`
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
            endpoint: t.endpoint,
            apiKey: t.apiKey,
            model: t.model,
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
    order: 20,
  },

  disableSearchInput: true,
  windowHeight: 'auto',
  mainView: () => TranslateView,
  searchBarAccessory: () => TranslateActions,
  subviews: { config: () => TranslateSettings },
  setup: async () => {
    await listen<string>('translate-text-ready', (e) => {
      if (translateReadyResolver) {
        translateReadyResolver(e.payload || '')
        translateReadyResolver = null
      }
    })
    // 跨扩展通信（C9）：screenshot OCR 等通过事件总线投递待翻译文本，
    // 避免扩展之间直 import 内部状态。
    await listen<string>('translate-pending-text', (e) => {
      pendingText.value = e.payload || ''
    })
    await initStreamListeners()
  },
  globalShortcuts: [
    {
      id: 'translate',
      default: 'Alt+T',
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
