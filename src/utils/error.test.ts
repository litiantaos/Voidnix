import { describe, it, expect } from 'vitest'
import { toErrorMessage } from './error'

describe('toErrorMessage', () => {
  it('Error 实例返回 message', () => {
    expect(toErrorMessage(new Error('网络超时'))).toBe('网络超时')
  })

  it('Error 实例无 message 时返回 fallback', () => {
    expect(toErrorMessage(new Error(''), '默认')).toBe('默认')
  })

  it('非 Error 类型返回 fallback', () => {
    expect(toErrorMessage('string', '默认')).toBe('默认')
    expect(toErrorMessage(123, '默认')).toBe('默认')
    expect(toErrorMessage(null, '默认')).toBe('默认')
    expect(toErrorMessage(undefined, '默认')).toBe('默认')
  })

  it('默认 fallback 是"未知错误"', () => {
    expect(toErrorMessage(null)).toBe('未知错误')
  })
})
