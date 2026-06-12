import { registerModule } from '@/core/module-registry'
import { asyncView } from '@/core/async-view'
import type { AppModule } from '@/types/module'

const FinderExtView = asyncView(() => import('./View.vue'))

const keywords = ['finder', '访达', '右键', '菜单', '扩展', 'extension']

const mod: AppModule = {
  id: 'finder-ext',
  name: '访达右键菜单',
  description: '访达右键快捷操作',
  icon: 'i-ri-folder-add-line',
  keywords,
  order: 60,
  view: FinderExtView,
  onSearch: async () => [],
}

registerModule(mod)
