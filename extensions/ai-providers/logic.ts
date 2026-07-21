import type { AiProvider } from '@/runtime/ai-providers'
import { apiKeyOf, providerDisplayName } from '@/runtime/ai-providers'

export function shellSingleQuote(value: string): string {
  return `'${value.replace(/'/g, `'\\''`)}'`
}

export function baseUrlEnvName(apiKeyEnv: string): string {
  const k = apiKeyEnv.trim()
  if (!k) return ''
  if (k.endsWith('_API_KEY')) return `${k.slice(0, -'_API_KEY'.length)}_BASE_URL`
  if (k.endsWith('_KEY')) return `${k.slice(0, -'_KEY'.length)}_BASE_URL`
  return ''
}

/** 智谱 Coding Plan Anthropic 兼容端（Claude Code 等）。 */
export const ZHIPU_ANTHROPIC_BASE_URL = 'https://open.bigmodel.cn/api/anthropic'

export interface ExportInput {
  providers: AiProvider[]
}

export interface ExportPayload {
  envText: string
}

/** 是否智谱 Coding Plan 端点（OpenCode / Grok 读 ZHIPU_API_KEY）。 */
export function isZhipuCodingEndpoint(endpoint: string): boolean {
  return /bigmodel\.cn|zhipuai/i.test(endpoint)
}

/** 是否 DeepSeek 端点（OpenCode / Grok 读 DEEPSEEK_API_KEY）。 */
export function isDeepseekEndpoint(endpoint: string): boolean {
  return /deepseek\.com/i.test(endpoint)
}

/**
 * 导出用 API Key 环境变量名。
 * 知名端点锁死工具约定名（OpenCode / Grok Build / Claude Code）；
 * 其余按名称或 hostname 推导。
 */
export function resolveEnvKey(p: AiProvider): string {
  if (p.envKey.trim()) return p.envKey.trim()
  if (isZhipuCodingEndpoint(p.endpoint)) return 'ZHIPU_API_KEY'
  if (isDeepseekEndpoint(p.endpoint)) return 'DEEPSEEK_API_KEY'
  const label = providerDisplayName(p)
  if (!label || label === '未命名提供商') return ''
  if (label === 'OPENAI') return ''
  return `${label.replace(/[^A-Za-z0-9]/g, '_').toUpperCase()}_API_KEY`
}

/** Claude Code 默认模型：glm-5.2 → glm-5.2[1M]。 */
export function anthropicModelFromZhipu(models: string[]): string {
  const m = models.find((x) => x.trim())?.trim() || 'glm-5.2'
  if (/\[/.test(m)) return m
  if (/^glm/i.test(m)) return `${m}[1M]`
  return m
}

/**
 * 备注 → env 后缀：仅 A-Za-z0-9，折叠下划线；纯中文等 → 空串（调用方回退 KEY{n}）。
 */
export function envLabelTag(label: string): string {
  return label
    .trim()
    .replace(/[^A-Za-z0-9]+/g, '_')
    .replace(/^_+|_+$/g, '')
    .replace(/_+/g, '_')
    .toUpperCase()
}

/** 导出 env 前缀（无 `_API_KEY`）；知名端点 → ZHIPU/DEEPSEEK，否则名称/域名，兜底 AI。 */
export function exportKeyPrefix(p: AiProvider): string {
  const canonical = resolveEnvKey(p)
  if (canonical) {
    const pre = canonical.replace(/_API_KEY$/, '')
    return pre || 'AI'
  }
  const label = providerDisplayName(p)
  if (label && label !== '未命名提供商') {
    const t = envLabelTag(label)
    if (t) return t
  }
  return 'AI'
}

/**
 * 为提供商每把非空 Key 分配互不冲突的 `*_API_KEY` 名。
 * - 单 Key：规范名（`DEEPSEEK_API_KEY` 等；OPENAI 端 resolve 为空则不导出 named）；
 *   规范名已被占用时序号兜底，**不静默丢**
 * - 多 Key：第一把非空拿规范名（有则）；其余 `PREFIX_TAG_API_KEY`；
 *   tag 来自备注 ASCII，空或碰撞则 `KEY{n}` / 递增后缀，**不静默丢 Key**
 */
export function assignKeyEnvNames(
  p: AiProvider,
  taken: Set<string> = new Set(),
): { keyId: string; envName: string }[] {
  const slots = (p.keys ?? []).filter((k) => k.apiKey.trim())
  if (slots.length === 0) return []

  const canonical = resolveEnvKey(p)
  const prefix = exportKeyPrefix(p)
  const out: { keyId: string; envName: string }[] = []
  let ordinal = 0

  for (const slot of slots) {
    ordinal += 1
    let envName = ''

    if (slots.length === 1) {
      // 单 Key：优先规范名；OPENAI 等 resolve 为空时跳过 named（已有 OPENAI_*）
      // 规范名已被 taken 时不 continue，落入下方序号兜底（两套同端点单 Key 不丢）
      if (!canonical) continue
      if (!taken.has(canonical)) envName = canonical
    } else if (ordinal === 1 && canonical && !taken.has(canonical)) {
      // 多 Key 第一把 → 工具约定名（OpenCode / Grok）
      envName = canonical
    } else {
      let tag = envLabelTag(slot.label)
      if (!tag) tag = `KEY${ordinal}`
      envName = `${prefix}_${tag}_API_KEY`
      if (taken.has(envName)) {
        let n = 2
        while (taken.has(`${prefix}_${tag}${n}_API_KEY`)) n += 1
        envName = `${prefix}_${tag}${n}_API_KEY`
      }
    }

    // 规范名已被占用 / 空：序号兜底，保证每把非空 Key 都写出
    if (!envName || taken.has(envName)) {
      let n = ordinal
      do {
        envName = `${prefix}_KEY${n}_API_KEY`
        n += 1
      } while (taken.has(envName))
    }

    taken.add(envName)
    out.push({ keyId: slot.id, envName })
  }
  return out
}

/**
 * 写出全部已配置凭证。
 * OPENAI_* 取**列表第一套完整** endpoint+key+model，仅方便只读 OPENAI_* 的 CLI；
 * 不是中枢「使用中」状态（中枢不维护 active）。
 *
 * 工具约定：
 * - OpenCode：`ZHIPU_API_KEY` / `DEEPSEEK_API_KEY`（+ opencode.json baseURL）
 * - Grok Build：`env_key` 同上
 * - Claude Code：智谱存在时写 `ANTHROPIC_*` → Zhipu Anthropic 兼容端
 */
export function buildExportPayload(input: ExportInput): ExportPayload {
  const lines: string[] = [
    '# Managed by Voidnix — do not edit.',
    '# source ~/.config/voidnix/ai.env (release) / ~/.config/voidnix.dev/ai.env (debug)',
    '# Tools: OpenCode / Grok Build (ZHIPU_* DEEPSEEK_*) · Claude Code (ANTHROPIC_*)',
    '',
  ]

  const first = input.providers.find(
    (p) => p.endpoint.trim() && apiKeyOf(p) && p.models.some((m) => m.trim()),
  )
  if (first) {
    const key = apiKeyOf(first)
    const model = first.models.find((m) => m.trim()) ?? ''
    lines.push('# OPENAI_* = first complete provider (not a global "active" selection)')
    lines.push(`export OPENAI_API_KEY=${shellSingleQuote(key)}`)
    lines.push(`export OPENAI_BASE_URL=${shellSingleQuote(first.endpoint.trim())}`)
    if (model) lines.push(`export OPENAI_MODEL=${shellSingleQuote(model)}`)
    lines.push('')
  }

  const seen = new Set<string>(['OPENAI_API_KEY', 'OPENAI_BASE_URL', 'OPENAI_MODEL'])
  let namedHeader = false
  for (const p of input.providers) {
    const assigned = assignKeyEnvNames(p, seen)
    if (assigned.length === 0) continue
    const byId = new Map(assigned.map((a) => [a.keyId, a.envName]))
    for (const slot of p.keys ?? []) {
      if (!slot.apiKey.trim()) continue
      const envName = byId.get(slot.id)
      if (!envName) continue
      if (!namedHeader) {
        lines.push('# Named keys (OpenCode / Grok Build env_key)')
        namedHeader = true
      }
      lines.push(`export ${envName}=${shellSingleQuote(slot.apiKey.trim())}`)
      const baseEnv = baseUrlEnvName(envName)
      if (baseEnv && p.endpoint.trim() && !seen.has(baseEnv)) {
        seen.add(baseEnv)
        lines.push(`export ${baseEnv}=${shellSingleQuote(p.endpoint.trim())}`)
      }
    }
  }
  if (namedHeader) lines.push('')

  // Claude Code：智谱 Coding Plan → Anthropic 兼容端
  const zhipu = input.providers.find((p) => isZhipuCodingEndpoint(p.endpoint) && apiKeyOf(p))
  if (zhipu) {
    const zKey = apiKeyOf(zhipu)
    const anthModel = anthropicModelFromZhipu(zhipu.models)
    lines.push('# Claude Code → Zhipu Anthropic-compatible')
    lines.push(`export ANTHROPIC_AUTH_TOKEN=${shellSingleQuote(zKey)}`)
    lines.push(`export ANTHROPIC_BASE_URL=${shellSingleQuote(ZHIPU_ANTHROPIC_BASE_URL)}`)
    lines.push(`export ANTHROPIC_DEFAULT_SONNET_MODEL=${shellSingleQuote(anthModel)}`)
    lines.push(`export ANTHROPIC_DEFAULT_OPUS_MODEL=${shellSingleQuote(anthModel)}`)
    lines.push(`export ANTHROPIC_DEFAULT_HAIKU_MODEL=${shellSingleQuote(anthModel)}`)
    lines.push('')
  }

  return { envText: lines.join('\n') }
}

/** 副标题用：首尾保留、中间省略。 */
export function maskKey(key: string): string {
  const k = key.trim()
  if (!k) return ''
  if (k.length <= 10) return `${k.slice(0, 2)}…${k.slice(-2)}`
  return `${k.slice(0, 6)}…${k.slice(-4)}`
}

/**
 * 窗口剩余时间：按单位小数显示；无效时间戳用横杠。
 * 5h 窗 → `2.3h`；7d 窗 → `2.3d`。
 */
export function formatWindowRemain(
  nextResetTimeMs: number | undefined,
  unit: 'h' | 'd',
  now = Date.now(),
): string {
  if (
    typeof nextResetTimeMs !== 'number' ||
    !Number.isFinite(nextResetTimeMs) ||
    nextResetTimeMs <= 0
  ) {
    return '—'
  }
  const ms = nextResetTimeMs - now
  if (ms <= 0) return unit === 'h' ? '0h' : '0d'
  if (unit === 'h') {
    const h = ms / 3_600_000
    if (h < 0.1) return `${Math.max(1, Math.round(ms / 60_000))}m`
    return h >= 10 ? `${Math.round(h)}h` : `${h.toFixed(1)}h`
  }
  const d = ms / 86_400_000
  if (d < 0.1) {
    const h = ms / 3_600_000
    return h >= 10 ? `${Math.round(h)}h` : `${Math.max(0.1, h).toFixed(1)}h`
  }
  return d >= 10 ? `${Math.round(d)}d` : `${d.toFixed(1)}d`
}

/** 用量数字：1.2K / 45.3M / 1.2B */
export function formatCompactCount(n: number): string {
  if (!Number.isFinite(n) || n < 0) return '0'
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(1)}B`
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`
  return String(Math.round(n))
}

/**
 * 列表副标题：
 * `sk-… · MAX · 5h 12% / 2.3h · 7d 34% / 2.3d · 30d 1.2B tokens`
 * 重置时间缺失时用 `—`。
 */
export function formatKeyUsageSubtitle(
  apiKey: string,
  m: ZhipuMonitor | undefined,
  now = Date.now(),
): string {
  const parts: string[] = []
  const masked = maskKey(apiKey)
  if (masked) parts.push(masked)
  else parts.push('无 Key')
  if (!m || m.error) {
    if (m?.error) parts.push(m.error)
    return parts.join(' · ')
  }
  if (m.level && m.level !== 'unknown') parts.push(m.level.toUpperCase())
  if (m.fiveHour) {
    const rem = formatWindowRemain(m.fiveHour.nextResetTime, 'h', now)
    parts.push(`5h ${Math.round(m.fiveHour.percentage)}% / ${rem}`)
  }
  if (m.weekly) {
    const rem = formatWindowRemain(m.weekly.nextResetTime, 'd', now)
    parts.push(`7d ${Math.round(m.weekly.percentage)}% / ${rem}`)
  }
  if (
    m.fiveHour ||
    m.weekly ||
    m.tokensSeries.length > 0 ||
    m.totalCalls > 0 ||
    m.totalTokens > 0
  ) {
    parts.push(`30d ${formatCompactCount(m.totalTokens)} tokens`)
  }
  return parts.join(' · ')
}

/** 列表副标题：脱敏 Key + DeepSeek 余额（CNY/USD）。 */
export function formatDeepseekBalanceSubtitle(
  apiKey: string,
  m: DeepseekBalance | undefined,
): string {
  const parts: string[] = []
  const masked = maskKey(apiKey)
  if (masked) parts.push(masked)
  else parts.push('无 Key')
  if (!m || m.error) {
    if (m?.error) parts.push(m.error)
    return parts.join(' · ')
  }
  if (!m.isAvailable) parts.push('余额不足')
  for (const b of m.balanceInfos) {
    const cur =
      b.currency === 'CNY' ? '¥' : b.currency === 'USD' ? '$' : b.currency ? `${b.currency} ` : ''
    parts.push(`${cur}${b.totalBalance}`)
  }
  if (m.balanceInfos.length === 0 && m.isAvailable) parts.push('可用')
  return parts.join(' · ')
}

export interface ZhipuMonitor {
  kind: 'zhipu'
  level: string
  expired: boolean
  fiveHour?: { percentage: number; nextResetTime: number }
  weekly?: { percentage: number; nextResetTime: number }
  mcp?: {
    usage: number
    total: number
    remaining: number
    percentage: number
    nextResetTime: number
  }
  totalCalls: number
  totalTokens: number
  tokensSeries: number[]
  error?: string | null
}

export interface DeepseekBalanceInfo {
  currency: string
  totalBalance: string
  grantedBalance: string
  toppedUpBalance: string
}

export interface DeepseekBalance {
  kind: 'deepseek'
  isAvailable: boolean
  balanceInfos: DeepseekBalanceInfo[]
  error?: string | null
}

export type KeyMonitor = ZhipuMonitor | DeepseekBalance

/** 归一化 invoke 返回（camelCase；兼容 snake_case 兜底）。 */
export function normalizeZhipuMonitor(raw: Record<string, unknown>): ZhipuMonitor {
  const slice = (v: unknown) => {
    if (!v || typeof v !== 'object') return undefined
    const o = v as Record<string, unknown>
    const percentage = Number(o.percentage ?? 0)
    const nextResetTime = Number(o.nextResetTime ?? o.next_reset_time ?? 0)
    return { percentage, nextResetTime }
  }
  const seriesRaw = raw.tokensSeries ?? raw.tokens_series
  let series: number[] = []
  if (Array.isArray(seriesRaw)) {
    series = (seriesRaw as unknown[]).map((x) => {
      const n = typeof x === 'number' ? x : Number(x)
      return Number.isFinite(n) ? n : 0
    })
  } else if (seriesRaw && typeof seriesRaw === 'object') {
    // 伪数组 / 对象 map
    series = Object.keys(seriesRaw as object)
      .filter((k) => /^\d+$/.test(k))
      .sort((a, b) => Number(a) - Number(b))
      .map((k) => {
        const n = Number((seriesRaw as Record<string, unknown>)[k])
        return Number.isFinite(n) ? n : 0
      })
  }
  return {
    kind: 'zhipu',
    level: String(raw.level ?? 'unknown'),
    expired: !!raw.expired,
    fiveHour: slice(raw.fiveHour ?? raw.five_hour),
    weekly: slice(raw.weekly),
    totalCalls: Number(raw.totalCalls ?? raw.total_calls ?? 0) || 0,
    totalTokens: Number(raw.totalTokens ?? raw.total_tokens ?? 0) || 0,
    tokensSeries: series,
    error: (raw.error as string | null | undefined) ?? null,
  }
}

export function normalizeDeepseekBalance(raw: Record<string, unknown>): DeepseekBalance {
  const infosRaw = raw.balanceInfos ?? raw.balance_infos
  const balanceInfos: DeepseekBalanceInfo[] = []
  if (Array.isArray(infosRaw)) {
    for (const item of infosRaw) {
      if (!item || typeof item !== 'object') continue
      const o = item as Record<string, unknown>
      balanceInfos.push({
        currency: String(o.currency ?? ''),
        totalBalance: String(o.totalBalance ?? o.total_balance ?? '0'),
        grantedBalance: String(o.grantedBalance ?? o.granted_balance ?? '0'),
        toppedUpBalance: String(o.toppedUpBalance ?? o.topped_up_balance ?? '0'),
      })
    }
  }
  return {
    kind: 'deepseek',
    isAvailable: !!(raw.isAvailable ?? raw.is_available),
    balanceInfos,
    error: (raw.error as string | null | undefined) ?? null,
  }
}
