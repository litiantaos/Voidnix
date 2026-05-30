import { registerModule } from '@/core/module-registry'
import { asyncView } from '@/core/async-view'
import { keywordModuleSearch } from '@/core/module-helpers'
import type { AppModule } from '@/types/module'

const FinderExtView = asyncView(() => import('./View.vue'))

const keywords = [
  'finder',
  '访达',
  '右键',
  '菜单',
  '扩展',
  'extension',
]

const mod: AppModule = {
  id: 'finder-ext',
  name: '访达右键菜单',
  description: '在访达右键菜单中添加快捷操作',
  icon: 'i-ri-folder-add-line',
  keywords,
  order: 60,
  layout: { view: FinderExtView },
  onSearch: async (query) => keywordModuleSearch(mod, query),
}

registerModule(mod)
