/**
 * 解析「无时区段按 UTC」的毫秒时间戳，非法输入返回 NaN。
 * 统一收敛三类入库/系统格式的解析（显式偏移优先，无偏移按 UTC）：
 * - SQLite `datetime('now')`：「YYYY-MM-DD HH:MM:SS」（无偏移段，UTC）
 * - mdls/mdfind：「YYYY-MM-DD HH:MM:SS +0000」（带偏移；JSC 的 new Date 不认此格式，须转 ISO）
 * - 标准 ISO 8601（T 分隔，含 Z / ±HH:MM / ±HHMM）：同样收敛到此入口
 * 与存储端「时间一律 UTC」约定配套；展示层拿到 epoch 后再转本地时区。
 */
export function parseUtcMs(s: string): number {
  const m =
    /^\s*(\d{4}-\d{2}-\d{2})[T ](\d{2}:\d{2}(?::\d{2})?(?:\.\d+)?)\s*(Z|[+-]\d{2}:?\d{2})?\s*$/.exec(
      s,
    )
  if (!m) return new Date(s).getTime()
  const raw = m[3]
  const offset = !raw || raw === 'Z' ? '+00:00' : raw.replace(/^([+-]\d{2})(\d{2})$/, '$1:$2')
  return new Date(`${m[1]}T${m[2]}${offset}`).getTime()
}
