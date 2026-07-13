import { defineExtension } from '@/runtime/extension-registry'
import { makeToggleHandler } from '@/stores/app'
import FinderExtView from './View.vue'
import { FINDER_SHORTCUT } from './shortcuts'

export default defineExtension({
  meta: {
    id: 'finder-ext',
    name: '访达工具',
    description: '访达快捷操作（拷贝路径 / 终端 / 新建文件 / 隐藏文件）',
    icon: 'i-ri-folder-add-line',
    keywords: ['finder', '访达', '路径', '终端', '新建文件', '隐藏文件'],
    order: 130,
  },

  disableSearchInput: true,
  windowHeight: 'auto',
  mainView: () => FinderExtView,

  globalShortcuts: [
    {
      id: FINDER_SHORTCUT.id,
      default: FINDER_SHORTCUT.default,
      onExecute: makeToggleHandler('finder-ext'),
    },
  ],
})
