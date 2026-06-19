import { defineExtension } from '@/runtime/extension-registry'
import type { ProviderResult } from '@/runtime/types'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { isTauri, type RawSearchResult } from '@/utils/tauri'
import { frequencyBoost } from '@/utils/fuzzy'
import { listen } from '@tauri-apps/api/event'
import { recencyScore, toResult } from './logic'

const MIN_FILE_QUERY_LEN = 2

// ── 应用前端缓存 ──
let appListCache: ProviderResult[] | null = null
let iconsPending = false

listen('app-cache-updated', () => {
  appListCache = null
  iconsPending = false
}).catch(() => {})

async function getAppList(): Promise<ProviderResult[]> {
  if (appListCache && !iconsPending) return appListCache
  const raw = await invoke<RawSearchResult[]>(CMD.searchApps).catch(() => [])
  const items = raw.map((r) =>
    toResult(r, frequencyBoost(r.use_count ?? 0) + recencyScore(r.last_used) + 1),
  )
  appListCache = items
  iconsPending = items.some((item) => item.data?.kind === 'application' && !item.data?.icon)
  return items
}

export default defineExtension({
  meta: {
    id: 'search',
    name: '搜索',
    description: '应用与文件搜索',
    icon: 'i-ri-search-line',
    hidden: true,
    order: 999,
  },

  search: {
    dynamic: async (query): Promise<ProviderResult[]> => {
      if (!isTauri) return []
      const results: ProviderResult[] = []

      // 应用搜索（空查询也返回，作为默认启动屏）
      try {
        const apps = await getAppList()
        if (!query.trim()) {
          results.push(...apps)
        } else {
          results.push(...apps.filter((a) => a.title.toLowerCase().includes(query.toLowerCase())))
        }
      } catch (e) {
        console.error('[search] apps error:', e)
      }

      // 文件搜索（需 ≥2 字符，mdfind 慢且短查询噪声大）
      const trimmed = query.trim()
      if (trimmed.length >= MIN_FILE_QUERY_LEN) {
        try {
          const raw = await invoke<RawSearchResult[]>(CMD.searchFiles, { query }).catch(() => [])
          for (const r of raw) {
            const isFolder = r.kind === 'folder'
            results.push(toResult(r, frequencyBoost(r.use_count ?? 0) + (isFolder ? 80 : 0)))
          }
        } catch (e) {
          console.error('[search] files error:', e)
        }
      }

      return results
    },
  },

  onExecute: async (result) => {
    if (!isTauri) return
    const path = result.data?.path
    if (path) {
      await invoke(CMD.launchApp, { path })
    }
  },
})
