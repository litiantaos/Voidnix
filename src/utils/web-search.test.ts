import { describe, it, expect } from 'vitest'
import {
  parseWebSearchQuery,
  buildSearchUrl,
  buildWebSearchResult,
  buildOpenUrlResult,
} from './web-search'

describe('parseWebSearchQuery', () => {
  it('parses //google query', () => {
    const result = parseWebSearchQuery('//hello world')
    expect(result.type).toBe('search')
    expect(result.engine).toBe('google')
    expect(result.keyword).toBe('hello world')
  })

  it('parses //bing query', () => {
    const result = parseWebSearchQuery('//b hello')
    expect(result.type).toBe('search')
    expect(result.engine).toBe('bing')
    expect(result.keyword).toBe('hello')
  })

  it('parses empty // as google search with empty keyword', () => {
    const result = parseWebSearchQuery('//')
    expect(result.type).toBe('search')
    expect(result.engine).toBe('google')
    expect(result.keyword).toBe('')
  })

  it('parses https URL', () => {
    const result = parseWebSearchQuery('//https://example.com')
    expect(result.type).toBe('url')
    expect(result.url).toBe('https://example.com')
  })

  it('parses domain as URL', () => {
    const result = parseWebSearchQuery('//example.com')
    expect(result.type).toBe('url')
    expect(result.url).toBe('https://example.com')
  })

  it('parses domain with path as URL', () => {
    const result = parseWebSearchQuery('//example.com/path/to/page')
    expect(result.type).toBe('url')
    expect(result.url).toBe('https://example.com/path/to/page')
  })

  it('parses IP address as URL', () => {
    const result = parseWebSearchQuery('//192.168.1.1:8080')
    expect(result.type).toBe('url')
  })
})

describe('buildSearchUrl', () => {
  it('builds google search URL', () => {
    const url = buildSearchUrl({ type: 'search', engine: 'google', keyword: 'hello' })
    expect(url).toBe('https://www.google.com/search?q=hello')
  })

  it('builds bing search URL', () => {
    const url = buildSearchUrl({ type: 'search', engine: 'bing', keyword: 'hello' })
    expect(url).toBe('https://www.bing.com/search?q=hello')
  })

  it('returns URL directly for url type', () => {
    const url = buildSearchUrl({ type: 'url', keyword: '', url: 'https://example.com' })
    expect(url).toBe('https://example.com')
  })

  it('encodes special characters', () => {
    const url = buildSearchUrl({ type: 'search', engine: 'google', keyword: 'hello world & test' })
    expect(url).toBe('https://www.google.com/search?q=hello%20world%20%26%20test')
  })
})

describe('buildWebSearchResult', () => {
  it('builds google result', () => {
    const result = buildWebSearchResult({ type: 'search', engine: 'google', keyword: 'test' })
    expect(result.title).toBe('Google 搜索')
    expect(result.data?.kind).toBe('web-search')
  })

  it('builds bing result', () => {
    const result = buildWebSearchResult({ type: 'search', engine: 'bing', keyword: 'test' })
    expect(result.title).toBe('Bing 搜索')
  })
})

describe('buildOpenUrlResult', () => {
  it('builds URL result', () => {
    const result = buildOpenUrlResult('https://example.com')
    expect(result.title).toBe('打开链接')
    expect(result.description).toBe('https://example.com')
    expect(result.data?.kind).toBe('open-url')
  })
})
