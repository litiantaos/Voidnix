export function getParentPath(path: unknown): string {
  if (typeof path !== 'string' || !path) return ''
  const lastSlashIndex = path.lastIndexOf('/')
  if (lastSlashIndex === -1) return path
  if (lastSlashIndex === 0) return '/' // 根目录
  return path.substring(0, lastSlashIndex)
}

const BYTE_UNITS = ['B', 'KB', 'MB', 'GB', 'TB', 'PB'] as const
const BYTE_UNITS_COMPACT = ['B', 'K', 'M', 'G', 'T', 'P'] as const

export interface FormatBytesOptions {
  /** 小数位，默认 1；compact 下 G 及以上固定 2 位（对齐原 proxy 口径） */
  decimals?: number
  /** 非正数 / null / undefined 时返回值；默认标准 `'0 B'`、compact `'0B'` */
  empty?: string
  /** 紧凑单位无空格（B/K/M/G），proxy 流量用；默认 false → `1.2 KB` */
  compact?: boolean
}

/**
 * 字节数格式化（列表副标题 / 详情元数据 / 仪表盘 / 代理流量共用）。
 * 标准：`512 B` · `1.5 KB` · `2 GB`；compact：`512B` · `1.5K` · `2.00G`。
 */
export function formatBytes(bytes: number | null | undefined, opts?: FormatBytesOptions): string {
  const compact = opts?.compact ?? false
  const empty = opts?.empty ?? (compact ? '0B' : '0 B')
  if (bytes == null || !Number.isFinite(bytes) || bytes <= 0) return empty

  const units = compact ? BYTE_UNITS_COMPACT : BYTE_UNITS
  const k = 1024
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(k)), units.length - 1)

  if (i === 0) {
    const n = Math.round(bytes)
    return compact ? `${n}${units[0]}` : `${n} ${units[0]}`
  }

  const decimals = compact && i >= 3 ? 2 : (opts?.decimals ?? 1)
  const raw = bytes / Math.pow(k, i)
  // 标准模式 strip 尾零（1.0 → 1）；compact 保留 toFixed 宽度与原 proxy 一致
  const num = compact ? raw.toFixed(decimals) : String(parseFloat(raw.toFixed(decimals)))
  return compact ? `${num}${units[i]}` : `${num} ${units[i]}`
}

/**
 * 速率格式化（字节/秒）。标准空值 `0 KB/s`；compact 空值 `0B/s`。
 * 调用方无需再拼 `/s`。
 */
export function formatRate(
  bytesPerSec: number | null | undefined,
  opts?: FormatBytesOptions,
): string {
  const compact = opts?.compact ?? false
  if (bytesPerSec == null || !Number.isFinite(bytesPerSec) || bytesPerSec < 1) {
    return compact ? '0B/s' : '0 KB/s'
  }
  return `${formatBytes(bytesPerSec, opts)}/s`
}

export function formatPathParts(path: unknown): { head: string; tail: string } {
  if (typeof path !== 'string' || !path) return { head: '', tail: '' }

  const displayPath = path.replace(/^\/Users\/[^/]+/, '~')

  const lastSlashIndex = displayPath.lastIndexOf('/')

  if (lastSlashIndex === -1 || lastSlashIndex === 0) {
    return { head: displayPath, tail: '' }
  }

  return {
    head: displayPath.substring(0, lastSlashIndex + 1),
    tail: displayPath.substring(lastSlashIndex + 1),
  }
}

/** 统一错误文案：Error.message / 字符串透传 / 其余 fallback（Tauri invoke reject 常为 string）。 */
export function toErrorMessage(e: unknown, fallback = '未知错误'): string {
  if (typeof e === 'string') {
    const s = e.trim()
    return s || fallback
  }
  if (e instanceof Error) return e.message || fallback
  return fallback
}

export function providerLabelFromUrl(url: string, fallback: string): string {
  if (!url) return fallback
  try {
    const parts = new URL(url).hostname.split('.')
    if (parts.length >= 2) return parts[parts.length - 2].toUpperCase()
    return parts[0].toUpperCase()
  } catch {
    return fallback
  }
}
