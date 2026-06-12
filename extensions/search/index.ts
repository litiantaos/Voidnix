import { registerModule } from '@/core/module-registry'
import type { AppModule, SearchResult } from '@/types/module'
import { invoke } from '@tauri-apps/api/core'
import { commands } from '@/bindings'
import { isTauri, toSearchResults } from '@/utils/tauri'
import { scoreFields, frequencyBoost } from '@/utils/fuzzy'
import { listen } from '@tauri-apps/api/event'

const APP_BOOST = 300
const FILE_HIT_BASE = 0
const MIN_FILE_QUERY_LEN = 2

// ── 应用前端缓存 ──
let appListCache: SearchResult[] | null = null

// 监听 Rust 侧应用缓存变更（安装/卸载/移动应用时触发）
listen('app-cache-updated', () => {
  appListCache = null
}).catch(() => {})

async function getAppList(): Promise<SearchResult[]> {
  if (appListCache) return appListCache
  const raw = await commands.searchApps().catch(() => [])
  const items = toSearchResults(raw, 'search-apps')
  appListCache = items
  return items
}

function scoreApp(item: SearchResult, query: string): number {
  const useCount = (item.data?.useCount as number) ?? 0
  if (!query.trim()) {
    const freq = frequencyBoost(useCount)
    const recency = recencyScore(item.data?.lastUsed as string | null)
    return freq + recency + 1
  }
  const match = scoreFields([item.title], query)
  if (match <= 0) return 0
  return match + frequencyBoost(useCount)
}

function recencyScore(lastUsed: string | null): number {
  if (!lastUsed) return 0
  const hours = (Date.now() - new Date(lastUsed).getTime()) / 3600000
  if (hours < 0) return 300
  if (hours < 1) return 300
  if (hours < 24) return 200
  if (hours < 168) return 100
  if (hours < 720) return 50
  return 0
}

function scoreFile(item: SearchResult, query: string): number {
  const useCount = (item.data?.useCount as number) ?? 0
  const parent = (item.data?.parent as string | null) ?? undefined
  const isFolder = item.data?.kind === 'folder'
  const match = scoreFields([item.title, parent], query)
  if (match <= 0) return 0
  return FILE_HIT_BASE + match + frequencyBoost(useCount) + (isFolder ? 80 : 0)
}

const searchApps: AppModule = {
  id: 'search-apps',
  name: '应用搜索',
  description: '全局搜索并启动 macOS 应用程序',
  icon: 'i-ri-apps-2-line',
  keywords: ['app', 'launch', '应用', '启动', '打开', 'open'],
  order: 99,
  hidden: true,
  onSearch: async (query) => {
    if (!isTauri) return []
    try {
      const items = await getAppList()
      const scored = items
        .map((it) => ({ it, score: scoreApp(it, query) }))
        .filter((e) => e.score > 0)
        .sort((a, b) => b.score - a.score)
        .slice(0, query.trim() ? 30 : 50)
        .map(({ it, score }, i) => ({
          ...it,
          score: score + APP_BOOST + (50 - i),
        }))
      return scored
    } catch (e) {
      console.error('[search-apps-module] search error:', e)
      return []
    }
  },
  onExecute: async (result) => {
    if (!isTauri) return
    const path = result.data?.path
    if (path) {
      await invoke('launch_app', { path })
    }
  },
}

const searchFiles: AppModule = {
  id: 'search-files',
  name: '文件搜索',
  description: '全局检索并打开本地文件与文件夹',
  icon: 'i-ri-file-search-line',
  keywords: ['search', 'file', 'folder', '搜索', '查找', '文件', '文件夹'],
  order: 99,
  hidden: true,
  onSearch: async (query) => {
    if (!isTauri || !query.trim()) return []
    // 短查询跳过文件搜索（mdfind 返回结果过多且速度慢）
    if (query.trim().length < MIN_FILE_QUERY_LEN) return []
    try {
      const raw = await commands.searchFiles(query).catch(() => [])
      const items = toSearchResults(raw, 'search-files')
      return items
        .map((it) => ({ it, score: scoreFile(it, query) }))
        .filter((e) => e.score > 0)
        .sort((a, b) => b.score - a.score)
        .slice(0, 50)
        .map(({ it, score }) => ({ ...it, score }))
    } catch (e) {
      console.error('[search-files-module] error:', e)
      return []
    }
  },
  onExecute: async (result) => {
    if (!isTauri) return
    const path = result.data?.path
    if (path) {
      await invoke('launch_app', { path })
    }
  },
}

registerModule(searchApps)
registerModule(searchFiles)
