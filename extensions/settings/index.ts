import { registerModule } from '@/core/module-registry'
import { asyncView } from '@/core/async-view'
import { keywordModuleSearch } from '@/core/module-helpers'
import type { AppModule } from '@/types/module'

const SettingsView = asyncView(() => import('./View.vue'))

const keywords = ['settings', 'config', '快捷键', '设置', '配置']

const mod: AppModule = {
  id: 'settings',
  name: '设置',
  description: '应用设置',
  icon: 'i-ri-settings-3-line',
  keywords,
  shortcut: '⌘,',
  order: 999,
  layout: { view: SettingsView },
  onSearch: async (query) => keywordModuleSearch(mod, query),
}

registerModule(mod)
