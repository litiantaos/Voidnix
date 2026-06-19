import { describe, it, expect } from 'vitest'
import { isValidIpLike } from './logic'

describe('isValidIpLike', () => {
  it('识别合法 IPv4', () => {
    expect(isValidIpLike('8.8.8.8')).toBe(true)
    expect(isValidIpLike('192.168.1.1')).toBe(true)
    expect(isValidIpLike('255.255.255.255')).toBe(true)
  })

  it('识别合法 IPv6', () => {
    expect(isValidIpLike('::1')).toBe(true)
    expect(isValidIpLike('2001:db8::1')).toBe(true)
    expect(isValidIpLike('fe80::1')).toBe(true)
  })

  it('拒绝非 IP 文本', () => {
    expect(isValidIpLike('')).toBe(false)
    expect(isValidIpLike('hello')).toBe(false)
    expect(isValidIpLike('abc.def.ghi.jkl')).toBe(false)
    expect(isValidIpLike('999.999.999.999')).toBe(true) // 仅形态校验，不校验范围
  })

  it('拒绝过短的纯冒号片段', () => {
    expect(isValidIpLike(':')).toBe(false) // length < 2
  })
})
