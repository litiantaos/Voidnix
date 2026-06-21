import { defineConfig } from '@/runtime/storage'
import { generateRequestId } from '@/utils/id'

export interface TranslateApiConfig {
  id: string
  type: 'youdao' | 'ai'
  appKey: string
  appSecret: string
  endpoint: string
  apiKey: string
  models: string[]
  prompt: string
}

/// translate 扩展自管配置（持久化至 extensions/translate/config.json）。
/// configs 为多引擎并发集合（translateText 遍历每项独立翻译），无「激活」概念。
export const config = defineConfig('extensions/translate/config', {
  targetLang: 'zh',
  configs: [
    {
      id: generateRequestId(),
      type: 'youdao',
      appKey: '',
      appSecret: '',
      endpoint: '',
      apiKey: '',
      models: [],
      prompt: '',
    },
  ] as TranslateApiConfig[],
})

/// CRUD helpers（defineConfig reactive 数组变更自动持久化）
export function addTranslateConfig(): string {
  const id = generateRequestId()
  config.configs.push({
    id,
    type: 'ai',
    appKey: '',
    appSecret: '',
    endpoint: '',
    apiKey: '',
    models: [],
    prompt: '',
  })
  return id
}

export function updateTranslateConfig(id: string, partial: Partial<TranslateApiConfig>) {
  const c = config.configs.find((c) => c.id === id)
  if (c) Object.assign(c, partial)
}

export function removeTranslateConfig(id: string) {
  if (config.configs.length <= 1) return
  const idx = config.configs.findIndex((c) => c.id === id)
  if (idx === -1) return
  config.configs.splice(idx, 1)
}
