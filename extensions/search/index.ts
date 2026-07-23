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
// 事件驱动失效：app-cache-updated 事件（icon 后台批次就绪 / 应用列表变更）触发置 null，下次 dynamic 重拉。
// 不再用 iconsPending 轮询——避免启动初期 icon 未齐时每按键都全量 invoke search_apps。
let appListCache: ProviderResult[] | null = null

listen('app-cache-updated', () => {
  appListCache = null
}).catch(() => {})

async function getAppList(): Promise<ProviderResult[]> {
  if (appListCache) return appListCache
  const raw = await invoke<RawSearchResult[]>(CMD.searchApps).catch((e) => {
    console.error('[search] search_apps invoke failed:', e)
    return []
  })
  const items = raw.map((r) =>
    toResult(r, frequencyBoost(r.use_count ?? 0) + recencyScore(r.last_used) + 1),
  )
  appListCache = items
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
    dynamic: async (query, ctx): Promise<ProviderResult[]> => {
      if (!isTauri) return []
      const emit = ctx?.emit
      const results: ProviderResult[] = []

      // 应用搜索（空查询也返回，作为默认启动屏；非空全量返回，由框架 groupAndSort 含拼音统一打分，
      // 避免扩展层裸 substring 预过滤丢弃拼音命中——如「jsq」→「计算器」）
      // 流式：缓存命中立即 emit（应用秒出，不被下方 mdfind 文件搜索阻塞）；无 emit 时累积到返回值
      try {
        const apps = await getAppList()
        if (emit) emit(apps)
        else results.push(...apps)
      } catch (e) {
        console.error('[search] apps error:', e)
      }

      // 文件搜索（需 ≥2 字符，mdfind 慢且短查询噪声大）：异步补充返回，不阻塞应用首批
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
