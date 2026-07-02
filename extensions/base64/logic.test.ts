import { describe, it, expect } from 'vitest'
import { encodeBase64, decodeBase64 } from './logic'

describe('encodeBase64', () => {
  it('encodes ASCII text', () => {
    expect(encodeBase64('hello')).toBe('aGVsbG8=')
    expect(encodeBase64('test')).toBe('dGVzdA==')
  })

  it('encodes UTF-8 text', () => {
    expect(encodeBase64('你好')).toBe('5L2g5aW9')
  })

  it('encodes empty string', () => {
    expect(encodeBase64('')).toBe('')
  })
})

describe('decodeBase64', () => {
  it('decodes ASCII base64', () => {
    expect(decodeBase64('aGVsbG8=')).toBe('hello')
    expect(decodeBase64('dGVzdA==')).toBe('test')
  })

  it('decodes UTF-8 base64', () => {
    expect(decodeBase64('5L2g5aW9')).toBe('你好')
  })

  it('returns empty string for invalid input', () => {
    expect(decodeBase64('!!!invalid!!!')).toBe('')
  })
})

describe('encode → decode roundtrip', () => {
  it('preserves ASCII text', () => {
    const original = 'Hello, World! 123'
    expect(decodeBase64(encodeBase64(original))).toBe(original)
  })

  it('preserves Chinese text', () => {
    const original = '你好世界'
    expect(decodeBase64(encodeBase64(original))).toBe(original)
  })

  it('preserves mixed text', () => {
    const original = 'Hello 你好'
    expect(decodeBase64(encodeBase64(original))).toBe(original)
  })
})
