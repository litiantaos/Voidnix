/** 图片扩展纯逻辑：格式白名单、路径展示、输出路径生成、IPC 类型。 */

import { formatBytes as formatBytesShared } from '@/utils/format'

/** 支持的输入图片格式（macOS 原生解码覆盖）。 */
export const IMAGE_EXTENSIONS = [
  'png',
  'jpg',
  'jpeg',
  'heic',
  'heif',
  'webp',
  'tiff',
  'tif',
  'bmp',
  'gif',
] as const

export function fileNameFromPath(path: string): string {
  const parts = path.split(/[/\\]/)
  return parts[parts.length - 1] || path
}

export function displayPath(path: string): string {
  return path.replace(/^\/Users\/[^/]+/, '~')
}

/** 字节展示（空值 `—`）。 */
export function formatBytes(n: number): string {
  return formatBytesShared(n, { empty: '—' })
}

/** 从完整文件 data URL（image_read_preview 原样 base64）推算字节数；非 data URL 返回 null。 */
export function bytesFromDataUrl(dataUrl: string): number | null {
  const comma = dataUrl.indexOf(',')
  if (comma < 0 || !dataUrl.slice(0, comma).includes('base64')) return null
  const b64 = dataUrl.slice(comma + 1)
  const padding = b64.endsWith('==') ? 2 : b64.endsWith('=') ? 1 : 0
  return Math.floor((b64.length * 3) / 4) - padding
}

/** 解码 data URL 取 `宽×高`；解码失败（格式不受当前引擎支持）返回空串。 */
export function imageSizeOf(dataUrl: string): Promise<string> {
  return new Promise((resolve) => {
    const img = new Image()
    img.onload = () => resolve(img.naturalWidth ? `${img.naturalWidth}×${img.naturalHeight}` : '')
    img.onerror = () => resolve('')
    img.src = dataUrl
  })
}

/** 净化文件名片段（去控制字符与路径分隔）。 */
function sanitizeStem(stem: string): string {
  return stem.replace(/[\x00-\x1f\x7f/\\]/g, '_').trim()
}

/** 从路径提取目录与文件名（目录无尾斜杠）。 */
function splitPath(inputPath: string): { dir: string; name: string } {
  const sep = inputPath.lastIndexOf('/')
  return {
    dir: sep >= 0 ? inputPath.slice(0, sep) : '',
    name: sep >= 0 ? inputPath.slice(sep + 1) : inputPath,
  }
}

/**
 * 生成输出路径：`{stem}.{suffix}.png`。
 * 输出目录默认与源文件相同，可指定 outputDir 覆盖。
 * 前端无 fs 探测，直接返回基础路径（文件已存在时 Rust save_png_safely 覆盖）。
 */
export function buildOutputPath(inputPath: string, outputDir?: string, suffix = 'nobg'): string {
  const { dir: srcDir, name } = splitPath(inputPath)
  const dir = outputDir?.trim() ? outputDir.replace(/\/$/, '') : srcDir
  const dot = name.lastIndexOf('.')
  const stem = sanitizeStem(dot > 0 ? name.slice(0, dot) : name) || 'image'
  const base = `${stem}.${suffix}.png`
  return dir ? `${dir}/${base}` : base
}

// ─── IPC 边界类型（与 Rust 端 serde 序列化对应）──

/** image_remove_bg / image_stitch 命令返回。 */
export interface ImageResult {
  previewDataUrl: string
  tempPath: string
  width: number
  height: number
  sizeBytes: number
}

/** 工具（参数组模式行选择）。 */
export type Tool = 'removeBg' | 'stitch'

/** 拼接方向。 */
export type StitchDirection = 'vertical' | 'horizontal'

/** 统一尺寸策略（与 Rust 端 Resize enum serde 对应）。 */
export type Resize = { mode: 'width'; value: number } | { mode: 'height'; value: number }

/** 统一尺寸预设值（px）。 */
export const RESIZE_PRESETS = [500, 1000, 2000] as const
