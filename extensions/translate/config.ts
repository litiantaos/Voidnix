import { defineConfig, whenConfigReady } from '@/runtime/storage'
import {
  config as hubConfig,
  getProviderById,
  providerDisplayName,
  getEnvSnapshot,
  apiKeyOf,
  addAiProvider,
  onAiProvidersChange,
  formatSelectionKey,
  parseSelectionKey as parseHubSelectionKey,
} from '@/runtime/ai-providers'

/** 固定两项服务 id（列表不再增删） */
export const SERVICE_YOUDAO_ID = 'service-youdao'
export const SERVICE_AI_ID = 'service-ai'

/// 判别联合：type 是创建时确定的不可变 discriminator。
export interface YoudaoConfig {
  id: string
  type: 'youdao'
  appKey: string
  appSecret: string
}

/** 跨提供商选中的一个模型（可选 keyId，与中枢选用串对齐） */
export interface AiModelSelection {
  providerId: string
  /** 多 Key 时指定；省略则取第一把非空 Key */
  keyId?: string
  model: string
}

/// AI 引擎：多选中枢模型 + 提示词；凭证只在中枢。
export interface AiConfig {
  id: string
  type: 'ai'
  /** 跨提供商多选；空 = 不跑 AI 翻译 */
  selections: AiModelSelection[]
  prompt: string
}

export type TranslateApiConfig = YoudaoConfig | AiConfig

export function selectionKey(s: AiModelSelection): string {
  return formatSelectionKey(s.providerId, s.keyId ?? '', s.model)
}

export function parseSelectionKey(key: string): AiModelSelection | null {
  const { providerId, keyId, model } = parseHubSelectionKey(key)
  if (!providerId || !model) return null
  return keyId ? { providerId, keyId, model } : { providerId, model }
}

/// translate 扩展自管配置（持久化至 extensions/translate/config.json）。
/// 固定两项：有道 + AI 翻译（并发）；无动态增删服务。
/// schema 变更：启动时一次性导入遗留 endpoint/apiKey；仍可删磁盘 config.json 重建。
export const config = defineConfig('extensions/translate/config', {
  targetLang: 'zh',
  configs: [
    { id: SERVICE_YOUDAO_ID, type: 'youdao', appKey: '', appSecret: '' },
    { id: SERVICE_AI_ID, type: 'ai', selections: [], prompt: '' },
  ] as TranslateApiConfig[],
})

export function getYoudaoConfig(): YoudaoConfig {
  const c = config.configs.find((x) => x.type === 'youdao')
  if (c && c.type === 'youdao') return c
  const created: YoudaoConfig = {
    id: SERVICE_YOUDAO_ID,
    type: 'youdao',
    appKey: '',
    appSecret: '',
  }
  config.configs.unshift(created)
  return created
}

export function getAiConfig(): AiConfig {
  const c = config.configs.find((x) => x.type === 'ai')
  if (c && c.type === 'ai') {
    if (!Array.isArray(c.selections)) c.selections = []
    if (typeof c.prompt !== 'string') c.prompt = ''
    return c
  }
  const created: AiConfig = {
    id: SERVICE_AI_ID,
    type: 'ai',
    selections: [],
    prompt: '',
  }
  config.configs.push(created)
  return created
}

export function updateYoudaoConfig(partial: Partial<Omit<YoudaoConfig, 'id' | 'type'>>) {
  Object.assign(getYoudaoConfig(), partial)
}

export function updateAiConfig(partial: Partial<Omit<AiConfig, 'id' | 'type'>>) {
  Object.assign(getAiConfig(), partial)
}

/** 单次 AI 调用目标（已解析 endpoint/key）。 */
export interface AiTranslateTarget {
  endpoint: string
  apiKey: string
  model: string
  label: string
}

/**
 * 将 AI 配置的多选模型解析为可调用目标列表。
 * 同步版：env 需事先 refreshEnvSnapshot。
 * keyId 缺省时 apiKeyOf 取第一把非空 Key。
 */
export function resolveAiTargets(cfg: AiConfig): AiTranslateTarget[] {
  const env = getEnvSnapshot()
  const out: AiTranslateTarget[] = []
  const seen = new Set<string>()

  for (const sel of cfg.selections ?? []) {
    const model = sel.model?.trim()
    const providerId = sel.providerId?.trim()
    if (!model || !providerId) continue
    const dedupe = selectionKey({ providerId, keyId: sel.keyId, model })
    if (seen.has(dedupe)) continue
    seen.add(dedupe)

    const provider = getProviderById(providerId)
    const endpoint = (provider?.endpoint.trim() || env.endpoint).trim()
    const apiKey = (apiKeyOf(provider, sel.keyId) || env.apiKey).trim()
    if (!endpoint || !apiKey) continue

    const keyLabel = sel.keyId
      ? provider?.keys.find((k) => k.id === sel.keyId)?.label
      : undefined
    const base = provider ? providerDisplayName(provider) : '环境变量'
    out.push({
      endpoint,
      apiKey,
      model,
      label: keyLabel ? `${base} · ${keyLabel}` : base,
    })
  }

  // 未选任何模型时：仅 env 完整则可跑一发（CLI/临时）
  if (out.length === 0) {
    const endpoint = env.endpoint.trim()
    const apiKey = env.apiKey.trim()
    const model = env.model.trim()
    if (endpoint && apiKey && model) {
      out.push({ endpoint, apiKey, model, label: '环境变量' })
    }
  }

  return out
}

// 删提供商 / Key 时清悬空 selections
onAiProvidersChange((e) => {
  const ai = getAiConfig()
  if (e.kind === 'remove-provider') {
    ai.selections = ai.selections.filter((s) => s.providerId !== e.providerId)
  } else if (e.kind === 'remove-key') {
    ai.selections = ai.selections.filter(
      (s) => !(s.providerId === e.providerId && s.keyId === e.keyId),
    )
  }
})

/**
 * 一次性：旧 AI 引擎字段（endpoint/apiKey/models）→ 中枢 + selections；
 * 并 strip 对象上的遗留密钥字段，避免继续落盘。
 */
function migrateLegacyAiFields() {
  const ai = getAiConfig()
  for (const c of config.configs) {
    if (c.type !== 'ai') continue
    const legacy = c as AiConfig & {
      endpoint?: string
      apiKey?: string
      models?: string[]
    }
    const endpoint = typeof legacy.endpoint === 'string' ? legacy.endpoint.trim() : ''
    const apiKey = typeof legacy.apiKey === 'string' ? legacy.apiKey : ''
    const models = Array.isArray(legacy.models) ? legacy.models.map(String) : []

    if (endpoint || apiKey.trim()) {
      const found = endpoint
        ? hubConfig.providers.find((p) => p.endpoint.trim() === endpoint)
        : undefined
      const id =
        found?.id ??
        addAiProvider({
          endpoint,
          apiKey,
          models,
        })
      if (ai.selections.length === 0) {
        for (const m of models) {
          const model = m.trim()
          if (model) ai.selections.push({ providerId: id, model })
        }
      }
    }

    if ('endpoint' in legacy) delete (legacy as { endpoint?: string }).endpoint
    if ('apiKey' in legacy) delete (legacy as { apiKey?: string }).apiKey
    if ('models' in legacy) delete (legacy as { models?: string[] }).models
  }
}

void whenConfigReady('extensions/translate/config').then(() => {
  try {
    migrateLegacyAiFields()
  } catch (e) {
    console.warn('[translate] legacy AI migrate skipped:', e)
  }
})
