import type { AppModule, SearchResult } from '@/types/module'
import { getAllModules } from '@/core/module-registry'
import { useAppStore } from '@/stores/app'
import { hideWindow } from '@/utils/tauri'
import { scoreFields } from '@/utils/fuzzy'

export function moduleSelfResult(mod: AppModule): SearchResult {
  return {
    id: `${mod.id}-module`,
    title: mod.name,
    description: mod.description,
    module: mod.id,
    icon: mod.icon,
    score: 100,
    data: { kind: 'module', moduleId: mod.id },
  }
}

export function getVisibleModules(): AppModule[] {
  return getAllModules()
    .filter((m) => !m.hidden)
    .sort((a, b) => (a.order ?? 9999) - (b.order ?? 9999))
}

export function moduleToSearchResult(m: AppModule, score: number): SearchResult {
  return {
    id: `module-${m.id}`,
    title: m.name,
    description: m.description,
    icon: m.icon,
    module: 'system',
    score,
    data: { kind: 'module', moduleId: m.id },
  }
}

export async function keywordSearchAll(query: string): Promise<SearchResult[]> {
  if (!query.trim()) return []

  return getAllModules()
    .filter((m) => m.keywords.length > 0)
    .map((m) => ({ mod: m, score: scoreFields([m.name, m.description, ...m.keywords], query) }))
    .filter((item) => item.score > 0)
    .sort((a, b) => b.score - a.score)
    .map(({ mod: m, score }) => ({ ...moduleSelfResult(m), score: score + 500 }))
}

export function makeToggleHandler(moduleId: string, onActivate?: () => void) {
  return (wasVisible: boolean) => {
    const appStore = useAppStore()
    if (wasVisible && appStore.activeModuleId === moduleId) {
      hideWindow()
      return
    }
    appStore.setActiveModule(moduleId)
    appStore.setSearchQuery('')
    onActivate?.()
  }
}
