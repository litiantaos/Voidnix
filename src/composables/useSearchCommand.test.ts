import { describe, it, expect } from 'vitest'
import { parseWebSearchQuery } from './useSearchCommand.test-utils'

describe('parseWebSearchQuery', () => {
  describe('Google 搜索', () => {
    it('// 关键词 → Google 搜索', () => {
      const result = parseWebSearchQuery('//hello world')
      expect(result).toEqual({ type: 'search', engine: 'google', keyword: 'hello world' })
    })

    it('仅 // → Google 搜索（空关键词）', () => {
      const result = parseWebSearchQuery('//')
      expect(result).toEqual({ type: 'search', engine: 'google', keyword: '' })
    })
  })

  describe('Bing 搜索', () => {
    it('//b 关键词 → Bing 搜索', () => {
      const result = parseWebSearchQuery('//b hello')
      expect(result).toEqual({ type: 'search', engine: 'bing', keyword: 'hello' })
    })

    it('仅 //b → Bing 搜索（空关键词）', () => {
      const result = parseWebSearchQuery('//b')
      expect(result).toEqual({ type: 'search', engine: 'bing', keyword: '' })
    })
  })

  describe('URL 检测', () => {
    it('https:// 完整 URL', () => {
      const result = parseWebSearchQuery('//https://example.com')
      expect(result).toEqual({ type: 'url', keyword: '', url: 'https://example.com' })
    })

    it('域名自动补 https://', () => {
      const result = parseWebSearchQuery('//example.com')
      expect(result).toEqual({ type: 'url', keyword: '', url: 'https://example.com' })
    })

    it('带路径的域名', () => {
      const result = parseWebSearchQuery('//example.com/path/to/page')
      expect(result).toEqual({ type: 'url', keyword: '', url: 'https://example.com/path/to/page' })
    })

    it('IP 地址', () => {
      const result = parseWebSearchQuery('//192.168.1.1')
      expect(result).toEqual({ type: 'url', keyword: '', url: 'https://192.168.1.1' })
    })

    it('IP 带端口', () => {
      const result = parseWebSearchQuery('//192.168.1.1:8080')
      expect(result).toEqual({ type: 'url', keyword: '', url: 'https://192.168.1.1:8080' })
    })
  })
})
