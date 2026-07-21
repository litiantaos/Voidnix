/** 视频扩展纯逻辑：格式白名单、路径展示、元数据行。 */

import { formatBytes as formatBytesShared } from '@/utils/format'

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

export function fileNameFromPath(path: string): string {
  const parts = path.split(/[/\\]/)
  return parts[parts.length - 1] || path
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

/** 字节展示（空值 `—`；与 @/utils/format 共用实现）。 */
export function formatBytes(n: number): string {
  return formatBytesShared(n, { empty: '—' })
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

// ─── IPC 边界类型（与 Rust 端 serde 序列化对应）──

/** video_core_status 命令返回。 */
export interface CoreStatus {
  available: boolean
  source: string
  version: string
  downloading: boolean
}

/** video_probe 命令返回。 */
export interface VideoMeta {
  path: string
  durationSecs: number
  width: number
  height: number
  videoCodec: string
  audioCodec: string
  sizeBytes: number
  container: string
}

/** video_job_status 命令返回。 */
export interface JobSnapshot {
  busy: boolean
  lastOutput: string | null
  lastError: string | null
  lastPercent: number
}

/** video_run 的 Channel<VideoEvent> 事件（started/progress/done/error）。 */
export type VideoEvent =
  | { type: 'started'; outputPath: string }
  | { type: 'progress'; percent: number; timeSecs: number; speed: string }
  | { type: 'done'; outputPath: string; sizeBytes: number }
  | { type: 'error'; message: string }
