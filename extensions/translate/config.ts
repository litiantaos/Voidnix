import { defineConfig, whenConfigReady } from '@/runtime/storage'
import { t } from '@/runtime/i18n'
import {
  config as hubConfig,
  getProviderById,
  getKeySlot,
  providerDisplayName,
  getEnvSnapshot,
  apiKeyOf,
  addAiProvider,
  formatSelectionKey,
  parseSelectionKey as parseHubSelectionKey,
  isCredentialSelectionValid,
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

/**
 * 规范一条选用：校验中枢 + 补全 keyId（旧式无 keyId → 第一把非空 Key）。
 * 无效返回 null。
 */
export function canonicalizeAiSelection(s: AiModelSelection): AiModelSelection | null {
  const providerId = s.providerId?.trim() ?? ''
  const model = s.model?.trim() ?? ''
  if (!providerId || !model) return null
  if (!isCredentialSelectionValid({ providerId, keyId: s.keyId, model })) return null
  const p = getProviderById(providerId)
  if (!p) return null
  const slot = getKeySlot(p, s.keyId)
  if (!slot) return null
  // 统一带 keyId，避免「无 keyId」与「同 Key 有 keyId」双份并存
  return { providerId, keyId: slot.id, model }
}

/**
 * 读时：校验 + 规范化 keyId + 按 providerId::keyId::model 去重（热路径不写回）。
 * 修复旧配置里同一模型同时存在 legacy / 三段式两条的问题。
 */
export function effectiveAiSelections(
  selections: AiModelSelection[] | undefined | null,
): AiModelSelection[] {
  if (!Array.isArray(selections) || selections.length === 0) return []
  const seen = new Set<string>()
  const out: AiModelSelection[] = []
  for (const s of selections) {
    const n = canonicalizeAiSelection(s)
    if (!n) continue
    const k = selectionKey(n)
    if (seen.has(k)) continue
    seen.add(k)
    out.push(n)
  }
  return out
}

function selectionsEqual(a: AiModelSelection[], b: AiModelSelection[]): boolean {
  if (a.length !== b.length) return false
  return a.every((s, i) => {
    const t = b[i]!
    return (
      s.providerId === t.providerId && (s.keyId ?? '') === (t.keyId ?? '') && s.model === t.model
    )
  })
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

/** 写入时压一次悬空项（冷路径落盘干净）。 */
export function updateAiConfig(partial: Partial<Omit<AiConfig, 'id' | 'type'>>) {
  const ai = getAiConfig()
  if (partial.selections) {
    partial = { ...partial, selections: effectiveAiSelections(partial.selections) }
  }
  Object.assign(ai, partial)
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
 * 走 effective（与中枢不一致的选用跳过）；keyId 缺省时 apiKeyOf 取第一把非空 Key。
 */
export function resolveAiTargets(cfg: AiConfig): AiTranslateTarget[] {
  const env = getEnvSnapshot()
  const out: AiTranslateTarget[] = []
  const seen = new Set<string>()

  for (const sel of effectiveAiSelections(cfg.selections)) {
    const model = sel.model.trim()
    const providerId = sel.providerId.trim()
    const dedupe = selectionKey({ providerId, keyId: sel.keyId, model })
    if (seen.has(dedupe)) continue
    seen.add(dedupe)

    const provider = getProviderById(providerId)
    const endpoint = (provider?.endpoint.trim() || env.endpoint).trim()
    const apiKey = (apiKeyOf(provider, sel.keyId) || env.apiKey).trim()
    if (!endpoint || !apiKey) continue

    // 与 selectionDisplayLabel 一致：仅多 Key 时附加备注
    const multiKey = (provider?.keys?.length ?? 0) > 1
    const keyLabel =
      multiKey && sel.keyId
        ? provider?.keys.find((k) => k.id === sel.keyId)?.label?.trim()
        : undefined
    const base = provider ? providerDisplayName(provider) : t('translate.envVars')
    out.push({
      endpoint,
      apiKey,
      model,
      label: keyLabel ? `${base} · ${keyLabel}` : base,
    })
  }

  // 未选任何有效模型时：仅 env 完整则可跑一发（CLI/临时）
  if (out.length === 0) {
    const endpoint = env.endpoint.trim()
    const apiKey = env.apiKey.trim()
    const model = env.model.trim()
    if (endpoint && apiKey && model) {
      out.push({ endpoint, apiKey, model, label: t('translate.envVars') })
    }
  }

  return out
}

/**
 * 冷路径：写回规范 + 去重后的 selections（含补 keyId、去双份）。
 * 热路径请用 effectiveAiSelections，勿 deep watch 中枢。
 */
export function pruneAiSelections() {
  const ai = getAiConfig()
  const prev = ai.selections
  if (!Array.isArray(prev)) {
    ai.selections = []
    return
  }
  if (prev.length === 0) return
  const next = effectiveAiSelections(prev)
  if (!selectionsEqual(prev, next)) ai.selections = next
}

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

void Promise.all([
  whenConfigReady('extensions/translate/config'),
  whenConfigReady('config/ai-providers'),
]).then(() => {
  try {
    migrateLegacyAiFields()
    pruneAiSelections()
  } catch (e) {
    console.warn('[translate] legacy AI migrate / prune skipped:', e)
  }
})
