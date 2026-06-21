import { defineExtension } from '@/runtime/extension-registry'
import FinderExtView from './View.vue'

export default defineExtension({
  meta: {
    id: 'finder-ext',
    name: '访达右键菜单',
    description: '访达右键菜单快捷操作',
    icon: 'i-ri-folder-add-line',
    keywords: ['finder', '访达', '右键', '菜单', '扩展', 'extension'],
    order: 60,
  },

  mainView: () => FinderExtView,
})
