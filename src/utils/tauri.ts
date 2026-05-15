import type { SearchResult as BindingsSearchResult } from '@/bindings'
import type { SearchResult } from '@/types/module'
import { getCachedIcon, setCachedIcon } from './icon-cache'

export const isTauri =
  typeof window !== 'undefined' &&
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ !== undefined

/**
 * 将 Rust 侧的 SearchResult（来自 bindings）转换为前端 SearchResult。
 * 原 TauriSearchResult 接口已删除，改用自动生成的类型。
 */
export function toSearchResults(items: BindingsSearchResult[], moduleId: string): SearchResult[] {
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
      score: item.score ?? 0,
      data: { path: item.path, kind: item.kind, icon },
    }
  })
}

export function cacheIconFromResult(path: string, icon: string | null | undefined): void {
  if (path && icon) {
    setCachedIcon(path, icon)
  }
}
