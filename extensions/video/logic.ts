/** 视频扩展纯逻辑：命名、格式白名单、元数据展示。 */

export type VideoMode = 'compress' | 'convert' | 'extract-audio'
export type Quality = 'high' | 'balanced' | 'small'
export type Scale = 'original' | '1080' | '720' | '480'
export type OutputFormat = 'mp4' | 'mov' | 'mkv' | 'webm' | 'gif' | 'm4a' | 'mp3'

export const VIDEO_EXTENSIONS = [
  'mp4',
  'mov',
  'mkv',
  'webm',
  'avi',
  'm4v',
  'wmv',
  'flv',
  'ts',
  'mts',
  'm2ts',
  '3gp',
  'mpeg',
  'mpg',
] as const

export const FORMAT_BY_MODE: Record<VideoMode, OutputFormat[]> = {
  compress: ['mp4', 'mov', 'mkv'],
  convert: ['mp4', 'mov', 'mkv', 'webm', 'gif'],
  'extract-audio': ['m4a', 'mp3'],
}

export function actionLabel(mode: VideoMode, _format?: OutputFormat): string {
  if (mode === 'extract-audio') return 'audio'
  if (mode === 'compress') return 'compressed'
  return 'converted'
}

/** 输出文件名：`{stem}.{action}.{ext}` */
export function buildOutputName(stem: string, mode: VideoMode, format: OutputFormat): string {
  const safe = sanitizeStem(stem) || 'video'
  return `${safe}.${actionLabel(mode, format)}.${format}`
}

export function sanitizeStem(stem: string): string {
  return stem.replace(/[\u0000-\u001f/\\]/g, '_').trim()
}

export function fileNameFromPath(path: string): string {
  const parts = path.split(/[/\\]/)
  return parts[parts.length - 1] || path
}

export function stemFromPath(path: string): string {
  const name = fileNameFromPath(path)
  const i = name.lastIndexOf('.')
  return i > 0 ? name.slice(0, i) : name
}

export function displayPath(path: string): string {
  return path.replace(/^\/Users\/[^/]+/, '~')
}

export function formatDuration(secs: number): string {
  if (!Number.isFinite(secs) || secs <= 0) return '—'
  const s = Math.round(secs)
  const h = Math.floor(s / 3600)
  const m = Math.floor((s % 3600) / 60)
  const r = s % 60
  if (h > 0) return `${h}:${String(m).padStart(2, '0')}:${String(r).padStart(2, '0')}`
  return `${m}:${String(r).padStart(2, '0')}`
}

export function formatBytes(n: number): string {
  if (!Number.isFinite(n) || n <= 0) return '—'
  if (n < 1024) return `${n} B`
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`
  return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`
}

export function formatMetaLine(meta: {
  durationSecs: number
  width: number
  height: number
  videoCodec: string
  sizeBytes: number
}): string {
  const parts: string[] = []
  if (meta.width && meta.height) parts.push(`${meta.width}×${meta.height}`)
  parts.push(formatDuration(meta.durationSecs))
  if (meta.videoCodec) parts.push(meta.videoCodec)
  parts.push(formatBytes(meta.sizeBytes))
  return parts.join(' · ')
}
