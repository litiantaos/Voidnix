import { describe, it, expect } from 'vitest'
import { encodeBase64, decodeBase64, tryDecodeBase64 } from './logic'

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

describe('tryDecodeBase64', () => {
  it('decodes valid base64 text', () => {
    expect(tryDecodeBase64('aGVsbG8=')).toBe('hello')
    expect(tryDecodeBase64('5L2g5aW9')).toBe('你好')
  })

  it('returns null for non-base64 characters', () => {
    expect(tryDecodeBase64('!!!invalid!!!')).toBeNull()
  })

  it('returns null for wrong length alignment', () => {
    expect(tryDecodeBase64('aGVsbG8')).toBeNull() // length 7, not % 4
  })

  it('returns null for empty result', () => {
    expect(tryDecodeBase64('')).toBeNull()
  })

  it('returns null for base64 decoding to binary control chars', () => {
    // AA== decodes to \x00 (null byte)
    expect(tryDecodeBase64('AA==')).toBeNull()
  })

  it('allows common whitespace in decoded text', () => {
    // "a\nb" encoded
    expect(tryDecodeBase64('YQpi')).toBe('a\nb')
  })

  it('respects minLength parameter', () => {
    expect(tryDecodeBase64('dGVzdA==', 12)).toBeNull() // length 8 < 12
    expect(tryDecodeBase64('dGVzdA==', 0)).toBe('test')
  })

  it('default minLength is 0', () => {
    expect(tryDecodeBase64('YQ==')).toBe('a')
  })
})
