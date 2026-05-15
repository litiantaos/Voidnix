import { registerModule } from '@/core/module-registry'
import type { AppModule } from '@/types/module'
import { commands } from '@/bindings'
import { isTauri, toSearchResults } from '@/utils/tauri'

const mod: AppModule = {
  id: 'search-files',
  name: '文件搜索',
  description: '全局检索并打开本地文件与文件夹',
  icon: 'i-ri-file-search-line',
  keywords: ['search', 'file', 'folder', '搜索', '查找', '文件', '文件夹'],
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
      const { invoke } = await import('@tauri-apps/api/core')
      await invoke('launch_app', { path })
    }
  },
}

registerModule(mod)
