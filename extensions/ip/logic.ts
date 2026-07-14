const IPV4_RE = /^\d{1,3}(\.\d{1,3}){3}$/
const IPV6_RE = /^[\da-fA-F:]+$/

/** 宽松判定字符串是否像 IPv4/IPv6（用于决定是否触发 IP 查询）。 */
export function isValidIpLike(s: string): boolean {
  if (IPV4_RE.test(s)) return true
  if (s.includes(':') && IPV6_RE.test(s) && s.length >= 2) return true
  return false
}
