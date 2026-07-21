import { scoreFields } from '@/utils/fuzzy'
import type { ClipboardItem } from './index'

export type ContentType = 'all' | 'text' | 'image' | 'file'

/** 提取用于匹配/索引的文本（图片/文件给语义占位，便于按类型搜索）。 */
export function matchText(item: ClipboardItem): string {
  if (item.content_type === 'image') return '图片 image'
  if (item.content_type === 'file') return `文件 file ${item.content}`
  return item.content
}

/** 列表标题：图片/文件给占位，文本截断 500 字符并压平换行。 */
export function clipboardTitle(item: ClipboardItem): string {
  if (item.content_type === 'image') return '[图片]'
  if (item.content_type === 'file') return '[文件] ' + item.content.split('/').pop()
  return item.content.substring(0, 500).replace(/\r?\n/g, ' ')
}

/** 列表图标：按内容类型选 iconify 图标类。 */
export function clipboardIcon(item: ClipboardItem): string {
  if (item.content_type === 'image') return 'i-ri-image-line'
  if (item.content_type === 'file') return 'i-ri-file-line'
  return 'i-ri-file-text-line'
}

/** 按 content_type 过滤（'all' 原样返回，其余返回新数组）。 */
export function filterByType(items: ClipboardItem[], type: ContentType): ClipboardItem[] {
  if (type === 'all') return items
  return items.filter((it) => it.content_type === type)
}

/** 按 query 模糊过滤 + 打分排序（score > 0 保留），空 query 原样返回。
 *  返回新数组（每项 score 字段已回填），不修改入参。 */
export function filterByQuery(items: ClipboardItem[], query: string): ClipboardItem[] {
  const q = query.trim()
  if (!q) return items
  return items
    .map((it) => ({ it, score: scoreFields([matchText(it)], q) }))
    .filter((e) => e.score > 0)
    .sort((a, b) => b.score - a.score)
    .map(({ it, score }) => ({ ...it, score }))
}
