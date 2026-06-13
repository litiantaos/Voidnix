import { invoke } from '@tauri-apps/api/core'
import type { SearchResult as BindingsSearchResult } from '@/bindings'
import type { SearchResult } from '@/types/module'

export function hideWindow(auto = false) {
  if (auto) {
    invoke('hide_window', { auto: true }).catch(() => {})
  } else {
    invoke('hide_window').catch(() => {})
  }
}

export const isTauri =
  typeof window !== 'undefined' &&
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ !== undefined

export function toSearchResults(items: BindingsSearchResult[], moduleId: string): SearchResult[] {
  return items.map((item) => {
    return {
      id: item.id,
      title: item.title,
      description: item.path,
      module: moduleId,
      score: item.score ?? 0,
      data: {
        path: item.path,
        kind: item.kind,
        icon: item.icon ?? null,
        useCount: item.use_count ?? 0,
        parent: item.parent ?? null,
        lastUsed: item.last_used ?? null,
      },
    }
  })
}
