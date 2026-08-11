import { ref } from 'vue'
import { defineExtension } from '@/runtime/extension-registry'
import { makeToggleHandler } from '@/stores/app'
import FinderExtView from './View.vue'
import { FINDER_SHORTCUT } from './shortcuts'
import './locales'

/** 快捷键进入 tick：窗口隐藏后重入时 onActivated 不触发，靠此信号驱动 View 重新探测选区。 */
export const reactivateTick = ref(0)

export default defineExtension({
  meta: {
    id: 'finder-ext',
    name: { 'zh-CN': '访达工具', en: 'Finder Tools' },
    description: {
      'zh-CN': '访达快捷操作（拷贝路径 / 终端 / 新建文件 / 隐藏文件）',
      en: 'Finder shortcuts (copy path / terminal / new file / toggle hidden)',
    },
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
      onExecute: makeToggleHandler('finder-ext', () => {
        reactivateTick.value++
      }),
    },
  ],
})
