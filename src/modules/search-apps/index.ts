import { registerModule } from '@/core/module-registry'
import type { AppModule } from '@/types/module'
import { invoke } from '@tauri-apps/api/core'
import { commands } from '@/bindings'
import { isTauri, toSearchResults } from '@/utils/tauri'

const mod: AppModule = {
  id: 'search-apps',
  name: '应用搜索',
  description: '全局搜索并启动 macOS 应用程序',
  icon: 'i-ri-apps-2-line',
  keywords: ['app', 'launch', '应用', '启动', '打开', 'open'],
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

registerModule(mod)
