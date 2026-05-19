import { defineAsyncComponent } from 'vue'
import { registerModule } from '@/core/module-registry'
import type { AppModule } from '@/types/module'

const FinderExtView = defineAsyncComponent(() => import('./View.vue'))

const mod: AppModule = {
  id: 'finder-ext',
  name: '访达右键菜单',
  description: '在访达右键菜单中添加拷贝路径、终端打开、切换隐藏文件等快捷操作',
  icon: 'i-ri-folder-add-line',
  keywords: ['finder', '访达', '右键', '菜单', '扩展', 'extension', 'finder extension'],
  order: 60,
  layout: { view: FinderExtView },
  onSearch: async () => {
    return []
  },
}

registerModule(mod)
