import type { AiProvider } from '@/runtime/ai-providers'
import { providerDisplayName } from '@/runtime/ai-providers'
import { t } from '@/runtime/i18n'

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

/** 是否智谱 Coding Plan 端点（固定后缀 VOIDNIX_ZHIPU）。 */
export function isZhipuCodingEndpoint(endpoint: string): boolean {
  return /bigmodel\.cn|zhipuai/i.test(endpoint)
}

/** 是否 DeepSeek 端点（固定后缀 VOIDNIX_DEEPSEEK）。 */
export function isDeepseekEndpoint(endpoint: string): boolean {
  return /deepseek\.com/i.test(endpoint)
}

export interface ExportInput {
  providers: AiProvider[]
}

export interface ExportPayload {
  envText: string
}

/**
 * 导出用 API Key 环境变量名。
 * 全量加 `VOIDNIX_` 私有前缀——不抢占外部工具约定的通用变量名（如 `ZHIPU_API_KEY`），
 * 外部工具必须显式引用 Voidnix 变量名。知名端点用固定后缀（ZHIPU / DEEPSEEK），
 * 其余按名称或 hostname 推导。`envKey` 显式优先（逃生舱，不加前缀）。
 */
export function resolveEnvKey(p: AiProvider): string {
  if (p.envKey.trim()) return p.envKey.trim()
  if (isZhipuCodingEndpoint(p.endpoint)) return 'VOIDNIX_ZHIPU_API_KEY'
  if (isDeepseekEndpoint(p.endpoint)) return 'VOIDNIX_DEEPSEEK_API_KEY'
  const label = providerDisplayName(p)
  if (!label || label === '未命名提供商') return ''
  return `VOIDNIX_${label.replace(/[^A-Za-z0-9]/g, '_').toUpperCase()}_API_KEY`
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

/** 导出前缀（无 `_API_KEY`）：canonical 非空时 `VOIDNIX_<名称/域名>`，否则空串。canonical 为空即信息不全，调用方整体跳过不导出。 */
export function exportKeyPrefix(p: AiProvider): string {
  const canonical = resolveEnvKey(p)
  return canonical.replace(/_API_KEY$/, '')
}

/**
 * 为提供商每把非空 Key 分配互不冲突的 `VOIDNIX_*_API_KEY` 名。
 * provider 信息不全（无 endpoint → canonical 为空）→ 整体不导出。
 * - 单 Key：规范名（`VOIDNIX_DEEPSEEK_API_KEY` 等）；规范名已被占用时序号兜底，**不静默丢**
 * - 多 Key：第一把非空拿规范名；其余 `VOIDNIX_PREFIX_TAG_API_KEY`；
 *   tag 来自备注 ASCII，空或碰撞则 `KEY{n}` / 递增后缀，**不静默丢 Key**
 */
export function assignKeyEnvNames(
  p: AiProvider,
  taken: Set<string> = new Set(),
): { keyId: string; envName: string }[] {
  const slots = (p.keys ?? []).filter((k) => k.apiKey.trim())
  if (slots.length === 0) return []

  const canonical = resolveEnvKey(p)
  if (!canonical) return []
  const prefix = exportKeyPrefix(p)
  const out: { keyId: string; envName: string }[] = []
  let ordinal = 0

  for (const slot of slots) {
    ordinal += 1
    let envName = ''

    if (slots.length === 1) {
      // 单 Key：规范名（已被 taken 则落入下方序号兜底，不丢）
      if (!taken.has(canonical)) envName = canonical
    } else if (ordinal === 1 && !taken.has(canonical)) {
      // 多 Key 第一把 → 规范名
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
 * 写出全部已配置凭证：每提供商一条 `VOIDNIX_*_BASE_URL`（先）+ 各 `VOIDNIX_*_API_KEY`（后）。
 * 全量私有前缀——不抢占外部工具约定的通用变量名，外部工具须显式引用。中枢不借用、不猜默认。
 */
export function buildExportPayload(input: ExportInput): ExportPayload {
  const lines: string[] = [
    '# Managed by Voidnix — do not edit.',
    '# source ~/.config/voidnix/ai.env (release) / ~/.config/voidnix.dev/ai.env (debug)',
    '# Private namespace (VOIDNIX_*) — external tools must reference explicitly',
    '',
  ]

  const seen = new Set<string>()
  const named: string[] = []
  for (const p of input.providers) {
    const assigned = assignKeyEnvNames(p, seen)
    if (assigned.length === 0) continue
    const byId = new Map(assigned.map((a) => [a.keyId, a.envName]))
    // BASE_URL 按「提供商」输出（endpoint 是提供商级属性，每提供商一条）；先 URL 再 Key
    const primaryEnv = assigned[0]?.envName ?? ''
    const baseEnv = primaryEnv ? baseUrlEnvName(primaryEnv) : ''
    if (baseEnv && p.endpoint.trim() && !seen.has(baseEnv)) {
      seen.add(baseEnv)
      named.push(`export ${baseEnv}=${shellSingleQuote(p.endpoint.trim())}`)
    }
    for (const slot of p.keys ?? []) {
      if (!slot.apiKey.trim()) continue
      const envName = byId.get(slot.id)
      if (!envName) continue
      named.push(`export ${envName}=${shellSingleQuote(slot.apiKey.trim())}`)
    }
  }
  if (named.length > 0) {
    lines.push('# VOIDNIX_ keys (external tools: reference explicitly)')
    lines.push(...named)
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
 * `sk-… · MAX · 5h 12% (2.3h) · 7d 34% (2.3d) · 30d 1.2B`
 * 重置时间缺失时用 `—`。仅为字符串拼装便利；视觉渲染走 {@link buildZhipuUsageSegments}。
 */
export function formatKeyUsageSubtitle(
  apiKey: string,
  m: ZhipuMonitor | undefined,
  now = Date.now(),
): string {
  return joinSegments(buildZhipuUsageSegments(apiKey, m, now))
}

/** 列表副标题：脱敏 Key + DeepSeek 余额（CNY/USD）。 */
export function formatDeepseekBalanceSubtitle(
  apiKey: string,
  m: DeepseekBalance | undefined,
): string {
  return joinSegments(buildDeepseekUsageSegments(apiKey, m))
}

/** 副标题片段语义色阶（与 theme.css 变量同源；accent 走字面值对齐 SparkLine）。 */
export type UsageTone =
  'muted' | 'secondary' | 'primary' | 'accent' | 'warning' | 'danger' | 'success'

export interface UsageSegment {
  text: string
  tone: UsageTone
  /** 紧贴前一段（空格连接，不插 `·`）；用于余量等次级信息视觉成组。 */
  lead?: boolean
}

/** 拼接片段为单字符串：lead 段用空格连接，其余用 ` · `。 */
function joinSegments(segs: UsageSegment[]): string {
  let s = ''
  for (let i = 0; i < segs.length; i++) {
    const seg = segs[i]!
    if (i === 0) s += seg.text
    else s += seg.lead ? ` ${seg.text}` : ` · ${seg.text}`
  }
  return s
}

/** 百分比 → tone：<70 primary / 70-89 warning / >=90 danger。 */
function percentageTone(p: number): UsageTone {
  if (p >= 90) return 'danger'
  if (p >= 70) return 'warning'
  return 'primary'
}

/**
 * 智谱 Key 副标题片段：
 * key=muted · 档位=accent · 5h/7d 百分比=阈值色 + 余量=muted lead · 30d 用量=primary · error=danger。
 */
export function buildZhipuUsageSegments(
  apiKey: string,
  m: ZhipuMonitor | undefined,
  now = Date.now(),
): UsageSegment[] {
  const out: UsageSegment[] = []
  const masked = maskKey(apiKey)
  out.push({ text: masked || t('ai-providers.noKey'), tone: 'muted' })
  if (!m || m.error) {
    if (m?.error) out.push({ text: m.error, tone: 'danger' })
    return out
  }
  if (m.level && m.level !== 'unknown') {
    out.push({ text: m.level.toUpperCase(), tone: 'accent' })
  }
  if (m.fiveHour) {
    const rem = formatWindowRemain(m.fiveHour.nextResetTime, 'h', now)
    out.push({
      text: `5h ${Math.round(m.fiveHour.percentage)}%`,
      tone: percentageTone(m.fiveHour.percentage),
    })
    out.push({ text: `(${rem})`, tone: 'muted', lead: true })
  }
  if (m.weekly) {
    const rem = formatWindowRemain(m.weekly.nextResetTime, 'd', now)
    out.push({
      text: `7d ${Math.round(m.weekly.percentage)}%`,
      tone: percentageTone(m.weekly.percentage),
    })
    out.push({ text: `(${rem})`, tone: 'muted', lead: true })
  }
  if (
    m.fiveHour ||
    m.weekly ||
    m.tokensSeries.length > 0 ||
    m.totalCalls > 0 ||
    m.totalTokens > 0
  ) {
    out.push({
      text: `30d ${formatCompactCount(m.totalTokens)}`,
      tone: 'primary',
    })
  }
  return out
}

/** DeepSeek 副标题片段：key=muted · 余额不足=danger · 余额数字=primary · 可用=success。 */
export function buildDeepseekUsageSegments(
  apiKey: string,
  m: DeepseekBalance | undefined,
): UsageSegment[] {
  const out: UsageSegment[] = []
  const masked = maskKey(apiKey)
  out.push({ text: masked || t('ai-providers.noKey'), tone: 'muted' })
  if (!m || m.error) {
    if (m?.error) out.push({ text: m.error, tone: 'danger' })
    return out
  }
  if (!m.isAvailable) out.push({ text: t('ai-providers.insufficientBalance'), tone: 'danger' })
  for (const b of m.balanceInfos) {
    const cur =
      b.currency === 'CNY' ? '¥' : b.currency === 'USD' ? '$' : b.currency ? `${b.currency} ` : ''
    out.push({ text: `${cur}${b.totalBalance}`, tone: 'primary' })
  }
  if (m.balanceInfos.length === 0 && m.isAvailable) {
    out.push({ text: t('ai-providers.available'), tone: 'success' })
  }
  return out
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
