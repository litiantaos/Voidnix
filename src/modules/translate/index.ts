import { ref } from 'vue'
import { defineAsyncComponent } from 'vue'
import { registerModule } from '@/core/module-registry'
import type { AppModule, SearchResult } from '@/types/module'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useSettingsStore, type TranslateApiConfig } from '@/stores/settings'
import { commands, type TranslateResult as BindingsTranslateResult } from '@/bindings'

const TranslateView = defineAsyncComponent(() => import('./View.vue'))
const TranslateSettings = defineAsyncComponent(() => import('./Settings.vue'))
const TranslateActions = defineAsyncComponent(() => import('./Actions.vue'))

/** 前端扩展类型：在 bindings 的 TranslateResult 基础上增加 loading 状态 */
export type TranslateResult = BindingsTranslateResult & { loading?: boolean }

export const translateResults = ref<TranslateResult[]>([])
export const isTranslating = ref(false)
export const sourceText = ref('')
export const pendingText = ref('')
export const inputText = ref('')

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
        commands.translateYoudao(text, config.appKey, config.appSecret, targetLang)
          .then((result) => {
            translateResults.value.splice(i, 1, result)
          })
          .catch((e) => {
            const msg = e instanceof Error ? e.message : String(e)
            translateResults.value.splice(i, 1, { source: text, translation: msg, engine })
          }),
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
        promises.push(
          commands.translateAi(text, config.endpoint, config.apiKey, model, targetLang, config.prompt ?? null)
            .then((result) => {
              result.engine += engineSuffix
              translateResults.value.splice(i, 1, result)
            })
            .catch((e) => {
              const msg = e instanceof Error ? e.message : String(e)
              translateResults.value.splice(i, 1, {
                source: text,
                translation: msg,
                engine,
              })
            }),
        )
      }
    }
  }

  translateResults.value = placeholder

  await Promise.all(promises)
  isTranslating.value = false
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

const mod: AppModule = {
  id: 'translate',
  name: '翻译',
  description: '选词翻译扩展',
  icon: 'i-ri-translate-2',
  keywords: ['translate', '翻译', '翻譯', 'fanyi', 'youdao', '有道'],
  order: 8,
  placeholder: '输入要翻译的文本，按回车翻译',
  layout: { view: TranslateView, searchBarAccessory: TranslateActions },
  settings: TranslateSettings,
  multiline: true,
  onSearch: async (query) => {
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
          commands.translateYoudao(query, config.appKey, config.appSecret, targetLang)
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
            commands.translateAi(query, config.endpoint, config.apiKey, model, targetLang, config.prompt ?? null)
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
                    activeModels.length > 1
                      ? `翻译 · ${model.trim()}`
                      : '翻译',
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

    // 如果没有配置翻译 API
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
