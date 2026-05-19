import { registerModule } from '@/core/module-registry'
import type { AppModule } from '@/types/module'
import { invoke } from '@tauri-apps/api/core'
import { commands } from '@/bindings'
import { isTauri, toSearchResults } from '@/utils/tauri'

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
      const apps = await commands.searchApps(query).catch(() => [])
      return toSearchResults(apps, 'search-apps')
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
    try {
      const files = await commands.searchFiles(query).catch(() => [])
      return toSearchResults(files, 'search-files')
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