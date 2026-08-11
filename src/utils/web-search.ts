import type { SearchResult } from '@/runtime/types'
import { t } from '@/runtime/i18n'

export interface WebSearchQuery {
  type: 'search' | 'url'
  engine?: 'google' | 'bing'
  keyword: string
  url?: string
}

/// 解析 `//query` / `//b query` / URL 输入。
export function parseWebSearchQuery(rawQuery: string): WebSearchQuery {
  const isBing = rawQuery.startsWith('//b ') || rawQuery === '//b'
  const keyword = rawQuery.startsWith('//b ')
    ? rawQuery.slice(4).trim()
    : rawQuery === '//b'
      ? ''
      : rawQuery.slice(2).trim()

  if (!keyword) return { type: 'search', engine: isBing ? 'bing' : 'google', keyword: '' }

  if (/^https?:\/\//.test(keyword)) {
    return { type: 'url', keyword: '', url: keyword }
  }

  if (
    /^[\w.-]+\.[a-z]{2,}(\/.*)?$/i.test(keyword) ||
    /^\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}(:\d+)?(\/.*)?$/.test(keyword)
  ) {
    return { type: 'url', keyword: '', url: `https://${keyword}` }
  }

  return { type: 'search', engine: isBing ? 'bing' : 'google', keyword }
}

/// 构建 web 搜索结果项。
export function buildWebSearchResult(parsed: WebSearchQuery): SearchResult {
  const engine = parsed.engine === 'bing' ? 'Bing' : 'Google'
  const desc = parsed.engine === 'bing' ? t('web.openInBrowser') : t('web.openInBrowserBing')
  return {
    id: 'web-search',
    title: t('web.search', { engine }),
    description: desc,
    icon: 'i-ri-earth-line',
    extId: 'system',
    data: { kind: 'web', engine: parsed.engine, keyword: parsed.keyword },
  }
}

/// 构建 URL 打开结果项。
export function buildOpenUrlResult(url: string): SearchResult {
  return {
    id: 'open-url',
    title: t('web.openLink'),
    description: url,
    icon: 'i-ri-links-line',
    extId: 'system',
    data: { kind: 'web', url },
  }
}

/// 构建 web 搜索的 URL（用于 open()）。
export function buildSearchUrl(parsed: WebSearchQuery): string {
  if (parsed.type === 'url') return parsed.url!
  return parsed.engine === 'bing'
    ? `https://www.bing.com/search?q=${encodeURIComponent(parsed.keyword)}`
    : `https://www.google.com/search?q=${encodeURIComponent(parsed.keyword)}`
}
