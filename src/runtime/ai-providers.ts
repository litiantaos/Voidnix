import { computed, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { defineConfig } from '@/runtime/storage'
import { generateRequestId } from '@/utils/id'
import { providerLabelFromUrl } from '@/utils/format'
import { isTauri } from '@/utils/tauri'
import { CMD } from '@/commands'

/** 额度/余额监控类型（可扩展）。 */
export type AiUsageKind = '' | 'zhipu-coding-plan' | 'deepseek-balance'

/** 同一提供商下的一把 Key（支持多号）。 */
export interface AiKeySlot {
  id: string
  /** 显示名，如「主号」「备用」 */
  label: string
  apiKey: string
}

/**
 * 配置中枢条目：只存 URL / keys / models，**不含「正在使用」**。
 * 选用由消费者（agent / translate 等）自行持有。
 */
export interface AiProvider {
  id: string
  /** 显示名；空则从 endpoint hostname 推导 */
  name: string
  endpoint: string
  models: string[]
  /** 多 Key；至少 1 项（CRUD 保底） */
  keys: AiKeySlot[]
  /**
   * 额度/余额监控。空 = 按 endpoint 自动识别
   *（bigmodel.cn → zhipu-coding-plan；deepseek.com → deepseek-balance）。
   */
  usageKind: AiUsageKind
  /** 导出到 shell 的额外 env 名；空则按 hostname 推导 */
  envKey: string
}

export interface ResolvedAiCredentials {
  endpoint: string
  apiKey: string
  model: string
  source: 'config' | 'env'
  providerId?: string
  keyId?: string
}

/** 消费者传入的选用（中枢本身不持久化 active）。 */
export interface AiCredentialSelection {
  providerId?: string
  keyId?: string
  model?: string
  env?: {
    apiKey?: string
    endpoint?: string
    model?: string
  }
}

export const config = defineConfig('config/ai-providers', {
  providers: [] as AiProvider[],
})

// ─── 规范化 / 迁移 ──────────────────────────────────────────

/** 旧 schema（单 apiKey / activeKeyId）→ keys[]。 */
export function normalizeProvider(raw: Record<string, unknown>): AiProvider {
  const id = typeof raw.id === 'string' ? raw.id : generateRequestId()
  const name = typeof raw.name === 'string' ? raw.name : ''
  const endpoint = typeof raw.endpoint === 'string' ? raw.endpoint : ''
  const models = Array.isArray(raw.models) ? (raw.models as string[]).map(String) : []
  const envKey = typeof raw.envKey === 'string' ? raw.envKey : ''
  const usageKind = (raw.usageKind as AiUsageKind) || ''

  if (Array.isArray(raw.keys) && raw.keys.length > 0) {
    const keys = (raw.keys as AiKeySlot[]).map((k) => ({
      id: k.id || generateRequestId(),
      label: k.label || 'Key',
      apiKey: k.apiKey || '',
    }))
    return { id, name, endpoint, models, keys, usageKind, envKey }
  }

  // legacy single apiKey
  const kid = generateRequestId()
  const legacyKey = typeof raw.apiKey === 'string' ? raw.apiKey : ''
  return {
    id,
    name,
    endpoint,
    models,
    keys: [{ id: kid, label: '默认', apiKey: legacyKey }],
    usageKind,
    envKey,
  }
}

export function newKeySlot(label = 'Key', apiKey = ''): AiKeySlot {
  return { id: generateRequestId(), label, apiKey }
}

// ─── 解析（按消费者传入的选用）──────────────────────────────

export function providerDisplayName(p: AiProvider): string {
  const n = p.name.trim()
  if (n) return n
  return providerLabelFromUrl(p.endpoint, '未命名提供商')
}

export function resolveUsageKind(p: AiProvider): AiUsageKind {
  if (p.usageKind) return p.usageKind
  if (/bigmodel\.cn|zhipuai/i.test(p.endpoint)) return 'zhipu-coding-plan'
  if (/deepseek\.com/i.test(p.endpoint)) return 'deepseek-balance'
  return ''
}

export function getProviderById(id: string): AiProvider | undefined {
  return config.providers.find((p) => p.id === id)
}

/** 取指定 key；无 keyId 时优先第一把非空 Key，否则第一把。 */
export function getKeySlot(p: AiProvider | undefined, keyId?: string): AiKeySlot | undefined {
  if (!p?.keys?.length) return undefined
  if (keyId) {
    const found = p.keys.find((k) => k.id === keyId)
    if (found) return found
  }
  return p.keys.find((k) => k.apiKey.trim()) ?? p.keys[0]
}

export function apiKeyOf(p: AiProvider | undefined, keyId?: string): string {
  return getKeySlot(p, keyId)?.apiKey.trim() ?? ''
}

/** @deprecated 用 apiKeyOf；保留别名避免零散引用炸裂 */
export const activeApiKey = apiKeyOf

/**
 * 选用串：`providerId::keyId::model`（推荐）或旧式 `providerId::model`（key 取第一把）。
 */
export function parseSelectionKey(key: string): {
  providerId: string
  keyId: string
  model: string
} {
  const parts = key.split('::')
  if (parts.length >= 3) {
    return {
      providerId: parts[0] ?? '',
      keyId: parts[1] ?? '',
      model: parts.slice(2).join('::').trim(),
    }
  }
  if (parts.length === 2) {
    return { providerId: parts[0] ?? '', keyId: '', model: (parts[1] ?? '').trim() }
  }
  return { providerId: '', keyId: '', model: '' }
}

export function formatSelectionKey(providerId: string, keyId: string, model: string): string {
  if (keyId) return `${providerId}::${keyId}::${model}`
  return `${providerId}::${model}`
}

/**
 * 消费者选用是否仍与中枢一致。
 * 提供商存在、模型仍在 models 列表；有 keyId 时该 Key 仍在。
 * （不要求 Key 非空——允许先占位后填密钥。）
 */
export function isCredentialSelectionValid(sel: {
  providerId?: string
  keyId?: string
  model?: string
}): boolean {
  const providerId = sel.providerId?.trim() ?? ''
  const model = sel.model?.trim() ?? ''
  if (!providerId || !model) return false
  const p = getProviderById(providerId)
  if (!p) return false
  if (!p.models.some((m) => m.trim() === model)) return false
  if (sel.keyId) {
    if (!p.keys.some((k) => k.id === sel.keyId)) return false
  }
  return true
}

/** 按选用解析凭证；未指定 providerId 时不猜「默认提供商」（仅 env 可补）。 */
export function resolveCredentials(sel: AiCredentialSelection = {}): ResolvedAiCredentials | null {
  const p = sel.providerId ? getProviderById(sel.providerId) : undefined
  const cfgEndpoint = p?.endpoint.trim() ?? ''
  const cfgKey = apiKeyOf(p, sel.keyId)
  const cfgModel = sel.model?.trim() ?? ''
  const envKey = sel.env?.apiKey?.trim() ?? ''
  const envEndpoint = sel.env?.endpoint?.trim() ?? ''
  const envModel = sel.env?.model?.trim() ?? ''

  const endpoint = cfgEndpoint || envEndpoint
  const apiKey = cfgKey || envKey
  const model = cfgModel || envModel
  if (!endpoint || !apiKey || !model) return null

  const fullyConfig = !!(cfgEndpoint && cfgKey && cfgModel)
  return {
    endpoint,
    apiKey,
    model,
    source: fullyConfig ? 'config' : 'env',
    providerId: p?.id,
    keyId: getKeySlot(p, sel.keyId)?.id,
  }
}

/** 列表里是否至少有一套可拼出 endpoint+key+model 的配置（给空态提示用）。 */
export const hasAnyConfiguredProvider = computed(() =>
  config.providers.some(
    (p) =>
      !!p.endpoint.trim() && p.keys.some((k) => k.apiKey.trim()) && p.models.some((m) => m.trim()),
  ),
)

// ─── Env 快照 ───────────────────────────────────────────────

export interface AiEnvSnapshot {
  apiKey: string
  endpoint: string
  model: string
  source: string
}

let envSnapshot: AiEnvSnapshot = {
  apiKey: '',
  endpoint: '',
  model: '',
  source: 'empty',
}

export function getEnvSnapshot(): AiEnvSnapshot {
  return envSnapshot
}

export async function refreshEnvSnapshot(): Promise<AiEnvSnapshot> {
  if (!isTauri) return envSnapshot
  try {
    envSnapshot = await invoke<AiEnvSnapshot>(CMD.aiProvidersEnvSnapshot)
  } catch (e) {
    console.error('[ai-providers] env snapshot failed:', e)
  }
  return envSnapshot
}

/** 加载后规范化 keys[]（legacy apiKey / 缺 keys）。 */
export function normalizeProvidersInPlace() {
  if (config.providers.length === 0) return
  const next = config.providers.map((p) =>
    normalizeProvider(p as unknown as Record<string, unknown>),
  )
  config.providers.splice(0, config.providers.length, ...next)
}

// ─── CRUD（只改配置，不写 active）──────────────────────────

export function addAiProvider(
  partial?: Partial<Omit<AiProvider, 'id'>> & { apiKey?: string; id?: string },
): string {
  // 迁移可传原 id；已存在则幂等返回（不覆盖现有条目）
  const requested = partial?.id?.trim()
  if (requested && getProviderById(requested)) return requested

  const id = requested || generateRequestId()
  let keys: AiKeySlot[]
  if (partial?.keys && partial.keys.length > 0) {
    keys = partial.keys.map((k) => ({
      id: k.id || generateRequestId(),
      label: k.label || 'Key',
      apiKey: k.apiKey || '',
    }))
  } else {
    keys = [newKeySlot('默认', partial?.apiKey ?? '')]
  }

  config.providers.push({
    id,
    name: partial?.name ?? '',
    endpoint: partial?.endpoint ?? '',
    models: partial?.models ? [...partial.models] : [],
    keys,
    usageKind: partial?.usageKind ?? '',
    envKey: partial?.envKey ?? '',
  })
  return id
}

export function removeAiProvider(id: string) {
  const idx = config.providers.findIndex((p) => p.id === id)
  if (idx === -1) return
  config.providers.splice(idx, 1)
}

export function updateAiProvider(id: string, partial: Partial<Omit<AiProvider, 'id'>>) {
  const target = config.providers.find((c) => c.id === id)
  if (!target) return
  Object.assign(target, partial)
}

export function addKeyToProvider(providerId: string, label = 'Key'): string {
  const p = getProviderById(providerId)
  if (!p) return ''
  const slot = newKeySlot(label)
  p.keys.push(slot)
  return slot.id
}

export function removeKeyFromProvider(providerId: string, keyId: string) {
  const p = getProviderById(providerId)
  if (!p || p.keys.length <= 1) return
  const idx = p.keys.findIndex((k) => k.id === keyId)
  if (idx === -1) return
  p.keys.splice(idx, 1)
}

/**
 * 选用展示文案：单 Key 仅模型；多 Key 为 `模型 · 备注`（折叠触发器 / 摘要可读）。
 */
export function selectionDisplayLabel(
  providerId: string,
  keyId: string | undefined,
  model: string,
): string {
  const m = model.trim()
  if (!m) return ''
  const p = getProviderById(providerId)
  if (!p || (p.keys?.length ?? 0) <= 1) return m
  const slot = keyId ? p.keys.find((k) => k.id === keyId) : undefined
  const tag = (slot?.label || '').trim() || (keyId ? 'Key' : '')
  return tag ? `${m} · ${tag}` : m
}

/** 是否存在任一提供商配置了多把 Key（UI 文案分支）。 */
export function hasMultiKeyProvider(): boolean {
  return config.providers.some((p) => (p.keys?.length ?? 0) > 1)
}

/**
 * 消费者下拉选项：`providerId::keyId::model`。
 * 分组 = 提供商名；单 Key 选项仅模型名；多 Key 选项为 `模型 · 备注`（与触发器一致）。
 */
export function modelSelectOptions():
  | { label: string; value: string }[]
  | { label: string; options: { label: string; value: string }[] }[] {
  type Opt = { label: string; value: string }
  type Group = { label: string; options: Opt[] }
  const groups: Group[] = []

  for (const p of config.providers) {
    const models = p.models.filter((m) => m.trim())
    if (models.length === 0) continue
    const keys = p.keys?.length ? p.keys : []
    if (keys.length === 0) continue

    if (keys.length === 1) {
      const k = keys[0]
      groups.push({
        label: providerDisplayName(p),
        options: models.map((m) => ({
          label: m,
          value: formatSelectionKey(p.id, k.id, m),
        })),
      })
    } else {
      // 多 Key：同提供商一组，选项显式带备注，避免折叠后只见模型名
      groups.push({
        label: providerDisplayName(p),
        options: keys.flatMap((k) =>
          models.map((m) => ({
            label: selectionDisplayLabel(p.id, k.id, m),
            value: formatSelectionKey(p.id, k.id, m),
          })),
        ),
      })
    }
  }

  if (groups.length === 0) return []
  if (groups.length === 1) return groups[0].options
  return groups
}

// ─── 粘贴出去（隐藏窗口 + Cmd+V）────────────────────────────

export async function pasteOut(text: string): Promise<void> {
  if (!text) throw new Error('内容为空')
  if (!isTauri) {
    await navigator.clipboard?.writeText(text)
    return
  }
  await invoke(CMD.pasteboardPasteText, { text })
}

// ─── 同步钩子 ───────────────────────────────────────────────

type SyncHandler = () => void
let syncHandler: SyncHandler | null = null
let syncWatchStarted = false

export function registerSyncHandler(handler: SyncHandler) {
  syncHandler = handler
  if (!syncWatchStarted) {
    syncWatchStarted = true
    watch(
      () => config.providers,
      () => {
        if (!syncHandler) return
        syncHandler()
      },
      { deep: true },
    )
  }
  handler()
}
