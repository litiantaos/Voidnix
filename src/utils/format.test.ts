import { describe, it, expect } from 'vitest'
import { getParentPath, formatPathParts } from './format'

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
