import { describe, it, expect } from 'vitest'
import { providerLabelFromUrl } from './provider'

describe('providerLabelFromUrl', () => {
  it('从 URL 提取域名主体', () => {
    expect(providerLabelFromUrl('https://www.google.com/search', '')).toBe('GOOGLE')
    expect(providerLabelFromUrl('https://github.com/user/repo', '')).toBe('GITHUB')
  })

  it('短域名取第一段', () => {
    expect(providerLabelFromUrl('https://localhost:3000', '')).toBe('LOCALHOST')
  })

  it('空 URL 返回 fallback', () => {
    expect(providerLabelFromUrl('', 'N/A')).toBe('N/A')
  })

  it('非法 URL 返回 fallback', () => {
    expect(providerLabelFromUrl('not a url', 'N/A')).toBe('N/A')
  })
})
