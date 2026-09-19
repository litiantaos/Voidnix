import { ref } from 'vue'
import { defineExtension } from '@/runtime/extension-registry'
import { makeToggleHandler } from '@/stores/app'
import FinderExtView from './View.vue'
import { FINDER_SHORTCUT } from './shortcuts'
import './locales'

/** 快捷键进入 tick：窗口隐藏后重入时 onActivated 不触发，靠此信号驱动 View 重新探测选区。 */
export const reactivateTick = ref(0)

/** 快捷键进入标记：makeToggleHandler 回调同步置位（先于 View 挂载与 onActivated），
 * View 据此区分进入方式——快捷键 = 访达上下文面板，应用界面进入 = 浏览模式。
 * 消费即复位（onActivated，或已激活态下重入的 tick watch），无跨次残留。 */
export const entryViaShortcut = ref(false)

export default defineExtension({
  meta: {
    id: 'finder-ext',
    name: { 'zh-CN': '访达工具', en: 'Finder Tools' },
    description: {
      'zh-CN': '访达快捷操作（拷贝路径 / 用 App 打开 / 终端 / 新建文件 / 隐藏文件）',
      en: 'Finder shortcuts (copy path / open with app / terminal / new file / toggle hidden)',
    },
    icon: 'i-ri-folder-add-line',
    keywords: ['finder', '访达', '路径', '终端', '新建文件', '隐藏文件', '打开方式', '编辑器'],
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
        entryViaShortcut.value = true
        reactivateTick.value++
      }),
    },
  ],
})
