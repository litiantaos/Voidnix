import { defineExtension } from '@/runtime/extension-registry'
import type { ProviderResult } from '@/runtime/types'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { isTauri, type RawSearchResult } from '@/utils/tauri'
import { frequencyBoost } from '@/utils/fuzzy'
import { listen } from '@tauri-apps/api/event'
import { recencyScore, toResult } from './logic'

const MIN_FILE_QUERY_LEN = 2

// ── 应用两层缓存：metadata（轻 ~20KB）+ icons（重 ~600KB base64）──
// search_apps 只返回元数据（无图标），get_app_icons 单独批量拉图标。
// app-cache-updated（应用增删）→ 失效 metadata；app-icons-updated（图标就绪）→ 仅失效图标。
let appMetaCache: ProviderResult[] | null = null
let appIconCache: Map<string, string> | null = null

listen('app-cache-updated', () => {
  appMetaCache = null
}).catch(() => {})

listen('app-icons-updated', () => {
  appIconCache = null
}).catch(() => {})

/** 将 appIconCache 中的图标 in-place patch 进 metadata 缓存（下次 emit 自动带上） */
function applyIcons(): void {
  if (!appMetaCache || !appIconCache) return
  for (const item of appMetaCache) {
    const icon = appIconCache.get(item.id)
    if (icon) {
      item.icon = icon
      if (item.data) (item.data as Record<string, unknown>).icon = icon
    }
  }
}

/** 批量拉取图标并合流进 metadata 缓存 */
async function fetchIcons(): Promise<void> {
  const icons = await invoke<{ id: string; icon: string | null }[]>(CMD.getAppIcons).catch((e) => {
    console.error('[search] get_app_icons invoke failed:', e)
    return []
  })
  appIconCache = new Map()
  for (const { id, icon } of icons) {
    if (icon) appIconCache.set(id, icon)
  }
  applyIcons()
}

async function getAppList(): Promise<ProviderResult[]> {
  if (appMetaCache && appIconCache) return appMetaCache

  if (!appMetaCache) {
    const raw = await invoke<RawSearchResult[]>(CMD.searchApps).catch((e) => {
      console.error('[search] search_apps invoke failed:', e)
      return []
    })
    appMetaCache = raw.map((r) =>
      toResult(r, frequencyBoost(r.use_count ?? 0) + recencyScore(r.last_used) + 1),
    )
    // 新 metadata 复用已有图标：app-cache-updated 仅失效 metadata，图标缓存仍有效
    applyIcons()
  }

  if (!appIconCache) {
    await fetchIcons()
  }

  return appMetaCache
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
      // 流式：缓存命中立即 emit（应用秒出，不被下方文件搜索阻塞）；无 emit 时累积到返回值
      try {
        const apps = await getAppList()
        if (emit) emit(apps)
        else results.push(...apps)
      } catch (e) {
        console.error('[search] apps error:', e)
      }

      // 文件搜索（内存索引，零延迟）：search_files 走 Rust 内存索引 substring 匹配，
      // 典型 ~3ms 返回（无 mdfind 子进程），与应用结果同步 emit 随打随出
      const trimmed = query.trim()
      if (trimmed.length >= MIN_FILE_QUERY_LEN) {
        try {
          const raw = await invoke<RawSearchResult[]>(CMD.searchFiles, { query }).catch(() => [])
          if (ctx?.signal.aborted) return results
          const fileResults: ProviderResult[] = []
          for (const r of raw) {
            const isFolder = r.kind === 'folder'
            fileResults.push(
              toResult(
                r,
                frequencyBoost(r.use_count ?? 0) + recencyScore(r.last_used) + (isFolder ? 240 : 0),
              ),
            )
          }
          if (emit && fileResults.length > 0) emit(fileResults)
          else results.push(...fileResults)
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
