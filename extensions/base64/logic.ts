export function encodeBase64(str: string): string {
  try {
    return btoa(
      encodeURIComponent(str).replace(/%([0-9A-F]{2})/g, (_, p1) =>
        String.fromCharCode(Number('0x' + p1)),
      ),
    )
  } catch {
    return ''
  }
}

export function decodeBase64(str: string): string {
  try {
    return decodeURIComponent(
      atob(str)
        .split('')
        .map((c) => '%' + ('00' + c.charCodeAt(0).toString(16)).slice(-2))
        .join(''),
    )
  } catch {
    return ''
  }
}

/** 尝试解码 base64：校验字符集 + 长度对齐 + 解码非空 + 结果可读（排除控制字符的二进制乱码）。
 *  minLength 门槛用于全局模式过滤短词误触（扩展模式传 0）。 */
export function tryDecodeBase64(input: string, minLength = 0): string | null {
  if (input.length < minLength) return null
  if (!/^[A-Za-z0-9+/=]+$/.test(input) || input.length % 4 !== 0) return null
  const decoded = decodeBase64(input)
  if (!decoded) return null
  // decodeBase64 已校验 UTF-8 有效性（decodeURIComponent），但有效 UTF-8 仍可能含
  // \x00-\x1F 控制字符，作为标题无意义
  for (const ch of decoded) {
    if (ch === '\n' || ch === '\r' || ch === '\t') continue
    const code = ch.charCodeAt(0)
    if (code < 32 || code === 127) return null
  }
  return decoded
}
