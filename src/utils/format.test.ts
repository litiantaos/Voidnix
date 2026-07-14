import { describe, it, expect } from 'vitest'
import {
  getParentPath,
  formatPathParts,
  formatBytes,
  formatRate,
  toErrorMessage,
  providerLabelFromUrl,
} from './format'

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

describe('formatBytes', () => {
  it('标准单位与空值', () => {
    expect(formatBytes(0)).toBe('0 B')
    expect(formatBytes(null)).toBe('0 B')
    expect(formatBytes(undefined)).toBe('0 B')
    expect(formatBytes(-1)).toBe('0 B')
    expect(formatBytes(512)).toBe('512 B')
    expect(formatBytes(1536)).toBe('1.5 KB')
    expect(formatBytes(1024 * 1024)).toBe('1 MB')
    expect(formatBytes(1024 ** 3)).toBe('1 GB')
  })

  it('empty 自定义 + decimals', () => {
    expect(formatBytes(null, { empty: '—' })).toBe('—')
    expect(formatBytes(0, { empty: '—' })).toBe('—')
    expect(formatBytes(1536, { decimals: 2 })).toBe('1.5 KB')
    expect(formatBytes(1600, { decimals: 2 })).toBe('1.56 KB')
  })

  it('compact 对齐原 proxy 口径', () => {
    expect(formatBytes(0, { compact: true })).toBe('0B')
    expect(formatBytes(512, { compact: true })).toBe('512B')
    expect(formatBytes(1536, { compact: true })).toBe('1.5K')
    expect(formatBytes(1024 * 1024, { compact: true })).toBe('1.0M')
    expect(formatBytes(1024 ** 3, { compact: true })).toBe('1.00G')
  })
})

describe('formatRate', () => {
  it('标准与 compact', () => {
    expect(formatRate(0)).toBe('0 KB/s')
    expect(formatRate(0.5)).toBe('0 KB/s')
    expect(formatRate(2048)).toBe('2 KB/s')
    expect(formatRate(0, { compact: true })).toBe('0B/s')
    expect(formatRate(1536, { compact: true })).toBe('1.5K/s')
  })
})

describe('toErrorMessage', () => {
  it('Error 对象提取 message', () => {
    expect(toErrorMessage(new Error('test'))).toBe('test')
  })

  it('字符串透传；空串/非 Error 用 fallback', () => {
    expect(toErrorMessage('粘贴失败')).toBe('粘贴失败')
    expect(toErrorMessage('  ')).toBe('未知错误')
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
