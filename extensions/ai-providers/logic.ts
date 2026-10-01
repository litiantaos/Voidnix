import { t } from '@/runtime/i18n'

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
