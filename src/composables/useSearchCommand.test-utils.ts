export interface WebSearchQuery {
  type: 'search' | 'url'
  engine?: 'google' | 'bing'
  keyword: string
  url?: string
}

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
