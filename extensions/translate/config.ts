import { defineConfig } from '@/runtime/storage'
import { generateRequestId } from '@/utils/id'

/// 判别联合：type 是创建时确定的不可变 discriminator，
/// 不同引擎只保存各自所需的字段，避免互补字段空值平铺。
export interface YoudaoConfig {
  id: string
  type: 'youdao'
  appKey: string
  appSecret: string
}

export interface AiConfig {
  id: string
  type: 'ai'
  endpoint: string
  apiKey: string
  models: string[]
  prompt: string
}

export type TranslateApiConfig = YoudaoConfig | AiConfig

/// translate 扩展自管配置（持久化至 extensions/translate/config.json）。
/// configs 为多引擎并发集合（translateText 遍历每项独立翻译），无「激活」概念。
export const config = defineConfig('extensions/translate/config', {
  targetLang: 'zh',
  configs: [
    { id: generateRequestId(), type: 'youdao', appKey: '', appSecret: '' },
  ] as TranslateApiConfig[],
})

/// CRUD helpers（defineConfig reactive 数组变更自动持久化）
/// 新增项默认为 ai 类型（用户主动添加的都是 ai；youdao 作为内置默认项始终存在）
export function addTranslateConfig(): string {
  const id = generateRequestId()
  config.configs.push({ id, type: 'ai', endpoint: '', apiKey: '', models: [], prompt: '' })
  return id
}

/// type 是不可变 discriminator，不参与更新；调用方按当前 config.type 传入对应字段子集
export function updateTranslateConfig(
  id: string,
  partial: Partial<Omit<YoudaoConfig, 'id' | 'type'>> | Partial<Omit<AiConfig, 'id' | 'type'>>,
) {
  const c = config.configs.find((c) => c.id === id)
  if (c) Object.assign(c, partial)
}

export function removeTranslateConfig(id: string) {
  if (config.configs.length <= 1) return
  const idx = config.configs.findIndex((c) => c.id === id)
  if (idx === -1) return
  config.configs.splice(idx, 1)
}
