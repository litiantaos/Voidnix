import type { ProviderResult, SearchResultKind } from '@/runtime/types'
import type { RawSearchResult } from '@/utils/tauri'

/** 最近使用时间衰减打分（分桶：<1h=300 / <24h=200 / <168h=100 / <720h=50 / else 0）。
 *  `now` 注入便于测试；默认 Date.now()。负值（未来时间）视作最近期。 */
export function recencyScore(lastUsed: string | null, now: number = Date.now()): number {
  if (!lastUsed) return 0
  const hours = (now - new Date(lastUsed).getTime()) / 3600000
  if (hours < 0) return 300
  if (hours < 1) return 300
  if (hours < 24) return 200
  if (hours < 168) return 100
  if (hours < 720) return 50
  return 0
}

/** Rust search_apps/search_files 原始项 → ProviderResult（kind 透传，module 由框架注入）。 */
export function toResult(raw: RawSearchResult, boost: number): ProviderResult {
  return {
    id: raw.id,
    title: raw.title,
    description: raw.path,
    icon: raw.icon ?? undefined,
    boost,
    data: {
      kind: (raw.kind as SearchResultKind) ?? 'file',
      path: raw.path,
      icon: raw.icon ?? undefined,
      useCount: raw.use_count ?? 0,
      parent: raw.parent ?? null,
      lastUsed: raw.last_used ?? null,
    },
  }
}
