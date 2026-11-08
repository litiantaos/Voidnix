/** 归档扩展前端类型与纯逻辑（Rust 侧手写镜像，见 native/）。 */

export type ArchiveFormat =
  'zip' | 'tar' | 'tarGz' | 'tarBz2' | 'tarXz' | 'tarZst' | 'sevenZ' | 'rar' | 'iso'

export type EntryKind = 'file' | 'dir' | 'symlink'

export interface EntryMeta {
  path: string
  kind: EntryKind
  size: number | null
}

export interface ArchiveInfo {
  path: string
  format: ArchiveFormat
  entries: EntryMeta[]
  totalSize: number
  encrypted: boolean
  truncated: boolean
}

export type CompressFormat = 'zip' | 'sevenZ'

export interface ExtractRequest {
  archivePath: string
  destDir: string
  /** null = 全部；目录条目隐含子树 */
  selected: string[] | null
  /** 解密密码（加密 zip；仅 zip 支持） */
  password: string | null
}

export interface ExtractResult {
  outputDir: string
  fileCount: number
  skippedUnsafe: number
  skippedSymlink: number
}

export interface CompressRequest {
  inputPaths: string[]
  outputPath: string
  format: CompressFormat
  level: number
  /** 过滤系统垃圾文件（.DS_Store / __MACOSX / ._前缀等） */
  junkFilter: boolean
  /** 过滤隐藏文件/文件夹（点开头组件，含 .git 等） */
  hiddenFilter: boolean
  /** 加密密码（zip / 7z：AES-256；tar.gz 格式忽略） */
  password: string | null
}

/** 压缩级别预设（用户语义而非数字） */
export const LEVEL_PRESETS = [
  { value: 1, key: 'archive.level.fastest' },
  { value: 6, key: 'archive.level.standard' },
  { value: 9, key: 'archive.level.smallest' },
] as const

export interface CompressResult {
  outputPath: string
  fileCount: number
  skippedJunk: number
  sizeBytes: number
}

/** archive_extract / archive_compress 的 Channel<ArchiveEvent> 事件。 */
export type ArchiveEvent =
  | { type: 'started'; totalBytes: number }
  | { type: 'progress'; percent: number }
  | { type: 'done'; outputPath: string; fileCount: number }
  | { type: 'error'; message: string }

export type PreviewPayload =
  | { type: 'text'; content: string }
  | { type: 'image'; mime: string; base64: string }
  | { type: 'binary' }

export interface EntryPreview {
  payload: PreviewPayload
  truncated: boolean
}

/** 可识别为归档的扩展名（finder-ext 投递路由 / 前端模式判定 / picker 过滤共用）。
 *  gz/bz2/xz/zst 单独出现不算（Rust detect 不认裸压缩流，仅 .tar.* 双扩展可解）。 */
export const ARCHIVE_EXTENSIONS = [
  'zip',
  'jar',
  'epub',
  '7z',
  'rar',
  'tar',
  'tgz',
  'tbz2',
  'txz',
  'tzst',
  'iso',
] as const

const ARCHIVE_EXT_SET: ReadonlySet<string> = new Set(ARCHIVE_EXTENSIONS)

/** 压缩流扩展名：仅 .tar.* 双扩展算归档（镜像 Rust format.rs::detect_by_ext）。 */
const TAR_STREAM_EXT: ReadonlySet<string> = new Set(['gz', 'bz2', 'xz', 'zst'])

export function isArchivePath(path: string): boolean {
  const name = path.split('/').pop() ?? ''
  const ext = extOf(name)
  if (ARCHIVE_EXT_SET.has(ext)) return true
  if (TAR_STREAM_EXT.has(ext)) {
    return name
      .slice(0, -(ext.length + 1))
      .toLowerCase()
      .endsWith('.tar')
  }
  return false
}

function extOf(path: string): string {
  const name = path.split('/').pop() ?? ''
  const dot = name.lastIndexOf('.')
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : ''
}

/** 归档显示名（去目录 + 截断长名）。 */
export function baseName(path: string): string {
  return path.split('/').pop() || path
}

/** 字节数人性化（B/KB/MB/GB，一位小数）。 */
export function formatBytes(n: number | null): string {
  if (n == null) return '—'
  if (n < 1024) return `${n} B`
  const units = ['KB', 'MB', 'GB', 'TB']
  let v = n / 1024
  let i = 0
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024
    i++
  }
  return `${v >= 100 ? Math.round(v) : v.toFixed(1)} ${units[i]}`
}

const COMPRESS_EXT: Record<CompressFormat, string> = {
  zip: 'zip',
  sevenZ: '7z',
}

/** 压缩输出名：单输入 `名.zip`；多输入 `归档.zip`；目录输入去尾斜杠。 */
export function defaultArchiveName(inputPaths: string[], format: CompressFormat): string {
  const ext = COMPRESS_EXT[format]
  const stem =
    inputPaths.length === 1
      ? baseName(inputPaths[0]).replace(/\.(tar\.)?[a-z0-9]+$/i, '') || 'archive'
      : 'archive'
  return `${stem}.${ext}`
}

/** 输出名规范化为当前格式的扩展名（去 .zip/.tar.gz/.7z 后重补，防「内容是 tar.gz 名字是 .zip」）。 */
export function normalizeOutputName(name: string, format: CompressFormat): string {
  const ext = `.${COMPRESS_EXT[format]}`
  const stem = name.trim().replace(/\.(?:tar\.gz|zip|7z)$/i, '') || 'archive'
  return `${stem}${ext}`
}

/** 格式徽标文案。 */
export const FORMAT_LABELS: Record<ArchiveFormat, string> = {
  zip: 'ZIP',
  tar: 'TAR',
  tarGz: 'TAR.GZ',
  tarBz2: 'TAR.BZ2',
  tarXz: 'TAR.XZ',
  tarZst: 'TAR.ZST',
  sevenZ: '7Z',
  rar: 'RAR',
  iso: 'ISO',
}
