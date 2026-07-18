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

export interface ExportInput {
  providers: AiProvider[]
}

export interface ExportPayload {
  envText: string
}

export function resolveEnvKey(p: AiProvider): string {
  if (p.envKey.trim()) return p.envKey.trim()
  const label = providerDisplayName(p)
  if (!label || label === '未命名提供商') return ''
  if (label === 'OPENAI') return ''
  return `${label.replace(/[^A-Za-z0-9]/g, '_').toUpperCase()}_API_KEY`
}

/**
 * 写出全部已配置凭证。
 * OPENAI_* 取**列表第一套完整** endpoint+key+model，仅方便只读 OPENAI_* 的 CLI；
 * 不是中枢「使用中」状态（中枢不维护 active）。
 */
export function buildExportPayload(input: ExportInput): ExportPayload {
  const lines: string[] = [
    '# Managed by Voidnix — do not edit.',
    '# source ~/.config/voidnix/ai.env',
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
    for (const slot of p.keys ?? []) {
      if (!slot.apiKey.trim()) continue
      let envName = ''
      if (p.keys.length > 1 && slot.label.trim()) {
        const base = resolveEnvKey(p) || `${providerDisplayName(p).toUpperCase()}_API_KEY`
        const prefix = base.replace(/_API_KEY$/, '')
        const tag = slot.label
          .trim()
          .replace(/[^A-Za-z0-9]+/g, '_')
          .toUpperCase()
        envName = `${prefix}_${tag}_API_KEY`
      } else {
        envName = resolveEnvKey(p)
      }
      if (!envName || seen.has(envName)) continue
      if (!namedHeader) {
        lines.push('# Named keys')
        namedHeader = true
      }
      seen.add(envName)
      lines.push(`export ${envName}=${shellSingleQuote(slot.apiKey.trim())}`)
      const baseEnv = baseUrlEnvName(envName)
      if (baseEnv && p.endpoint.trim() && !seen.has(baseEnv)) {
        seen.add(baseEnv)
        lines.push(`export ${baseEnv}=${shellSingleQuote(p.endpoint.trim())}`)
      }
    }
  }
  if (namedHeader) lines.push('')

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
  if (typeof nextResetTimeMs !== 'number' || !Number.isFinite(nextResetTimeMs) || nextResetTimeMs <= 0) {
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

/** @deprecated 用 formatWindowRemain */
export function formatResetCountdown(nextResetTimeMs: number, now = Date.now()): string {
  return formatWindowRemain(nextResetTimeMs, 'h', now)
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
  if (m.fiveHour || m.weekly || m.tokensSeries.length > 0 || m.totalCalls > 0 || m.totalTokens > 0) {
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
