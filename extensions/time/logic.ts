/** 数字补零（单位数前补 0）。 */
export function pad(n: number): string {
  return n < 10 ? '0' + n : String(n)
}

/** 本地 ISO 风格字符串（带时区偏移，如 2026-06-19T17:40:00+08:00）。 */
export function toLocalIso(date: Date): string {
  const off = -date.getTimezoneOffset()
  const sign = off >= 0 ? '+' : '-'
  const absOff = Math.abs(off)
  return (
    `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}` +
    `T${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}` +
    `${sign}${pad(Math.floor(absOff / 60))}:${pad(absOff % 60)}`
  )
}

/** 解析 Unix 时间戳（10 位秒 / 13 位毫秒）或日期字符串为毫秒时间戳。
 *  返回 { ts: 毫秒, isMs: 输入是否为毫秒级 }；空或无法解析返回 null。
 *  - 10 位纯数字 → 视为秒，isMs=false（ts 已归一为毫秒）
 *  - 13 位纯数字 → 视为毫秒，isMs=true
 *  - 其余 → 尝试作为日期字符串解析，成功则 isMs=true */
export function parseTimestamp(input: string): { ts: number; isMs: boolean } | null {
  const trimmed = input.trim()
  if (!trimmed) return null
  if (/^\d{10}$/.test(trimmed)) return { ts: parseInt(trimmed, 10) * 1000, isMs: false }
  if (/^\d{13}$/.test(trimmed)) return { ts: parseInt(trimmed, 10), isMs: true }
  const ms = new Date(trimmed).getTime()
  if (isNaN(ms)) return null
  return { ts: ms, isMs: true }
}
