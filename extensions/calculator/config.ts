import { defineConfig } from '@/runtime/storage'

export interface CalcHistoryEntry {
  expr: string
  result: string
}

/// calculator 扩展自管历史记录（持久化至 extensions/calculator/config.json）。
/// version: 1 —— schema 版本号，磁盘不匹配时清空用 defaults（自开发自用）。
export const config = defineConfig(
  'extensions/calculator/config',
  {
    history: [] as CalcHistoryEntry[],
  },
  { version: 1 },
)

/// 追加历史记录，自截为最近 10 条。
export function appendHistory(expr: string, result: string) {
  // 同表达式去重（最近一条相同则跳过）
  if (config.history.length > 0 && config.history[0].expr === expr) return
  config.history.unshift({ expr, result })
  if (config.history.length > 10) {
    config.history.splice(10)
  }
}
