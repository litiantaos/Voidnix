import type { SearchResult } from '@/types/module'
import { getCachedIcon, setCachedIcon } from './icon-cache'

export const isTauri =
  typeof window !== 'undefined' &&
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ !== undefined

export interface TauriSearchResult {
  id: string
  title: string
  path: string
  kind: string
  icon: string | null
  score?: number
}

export function toSearchResults(items: TauriSearchResult[], moduleId: string): SearchResult[] {
  return items.map((item) => {
    let icon = item.icon
    if (!icon && item.path && item.kind === 'application') {
      icon = getCachedIcon(item.path) ?? null
    }
    return {
      id: item.id,
      title: item.title,
      description: item.path,
      module: moduleId,
      score: item.score || 0,
      data: { path: item.path, kind: item.kind, icon },
    }
  })
}

export function cacheIconFromResult(path: string, icon: string | null | undefined): void {
  if (path && icon) {
    setCachedIcon(path, icon)
  }
}
