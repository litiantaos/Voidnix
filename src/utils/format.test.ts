import { describe, it, expect } from 'vitest'
import { getParentPath, formatPathParts, toErrorMessage, providerLabelFromUrl } from './format'

describe('getParentPath', () => {
  it('正常路径返回父目录', () => {
    expect(getParentPath('/Users/test/file.txt')).toBe('/Users/test')
    expect(getParentPath('/a/b/c')).toBe('/a/b')
  })

  it('根目录返回自身', () => {
    expect(getParentPath('/')).toBe('/')
    expect(getParentPath('/file.txt')).toBe('/')
  })

  it('无斜杠返回原路径', () => {
    expect(getParentPath('file.txt')).toBe('file.txt')
  })

  it('空值/非字符串安全处理', () => {
    expect(getParentPath('')).toBe('')
    expect(getParentPath(null)).toBe('')
    expect(getParentPath(undefined)).toBe('')
    expect(getParentPath(123)).toBe('')
  })
})

describe('formatPathParts', () => {
  it('拆分路径为 head + tail，/Users/xxx 替换为 ~', () => {
    expect(formatPathParts('/Users/test/docs/file.txt')).toEqual({
      head: '~/docs/',
      tail: 'file.txt',
    })
  })

  it('将 /Users/xxx 替换为 ~', () => {
    const result = formatPathParts('/Users/username/docs/readme.md')
    expect(result.head).toContain('~')
    expect(result.head).not.toContain('/Users/username')
  })

  it('根目录/无斜杠返回 head 为全路径', () => {
    expect(formatPathParts('/')).toEqual({ head: '/', tail: '' })
    expect(formatPathParts('file.txt')).toEqual({ head: 'file.txt', tail: '' })
  })

  it('空值/非字符串安全处理', () => {
    expect(formatPathParts('')).toEqual({ head: '', tail: '' })
    expect(formatPathParts(null)).toEqual({ head: '', tail: '' })
    expect(formatPathParts(undefined)).toEqual({ head: '', tail: '' })
  })
})

describe('toErrorMessage', () => {
  it('Error 对象提取 message', () => {
    expect(toErrorMessage(new Error('test'))).toBe('test')
  })

  it('非 Error 用 fallback', () => {
    expect(toErrorMessage('string')).toBe('未知错误')
    expect(toErrorMessage(42)).toBe('未知错误')
  })

  it('空 Error.message 用 fallback', () => {
    expect(toErrorMessage(new Error(''))).toBe('未知错误')
  })

  it('自定义 fallback', () => {
    expect(toErrorMessage(null, 'custom')).toBe('custom')
  })
})

describe('providerLabelFromUrl', () => {
  it('从 URL 提取域名主体', () => {
    expect(providerLabelFromUrl('https://api.openai.com/v1', 'AI')).toBe('OPENAI')
    expect(providerLabelFromUrl('https://api.tencentyun.com/', 'AI')).toBe('TENCENTYUN')
  })

  it('空 URL 返回 fallback', () => {
    expect(providerLabelFromUrl('', 'Fallback')).toBe('Fallback')
  })

  it('无效 URL 返回 fallback', () => {
    expect(providerLabelFromUrl('not-a-url', 'Fallback')).toBe('Fallback')
  })

  it('单段域名取首段', () => {
    expect(providerLabelFromUrl('https://localhost/v1', 'AI')).toBe('LOCALHOST')
  })
})
