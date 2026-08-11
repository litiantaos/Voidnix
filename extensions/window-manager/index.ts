import { defineExtension } from '@/runtime/extension-registry'
import './locales'
import WindowManagerView from './View.vue'

export default defineExtension({
  meta: {
    id: 'window-manager',
    name: { 'zh-CN': '窗口管理', en: 'Window Manager' },
    description: { 'zh-CN': '窗口布局与分屏', en: 'Window layout and snap tiling' },
    icon: 'i-ri-layout-grid-line',
    keywords: ['window', 'manager', 'layout', 'snap', 'tile', '窗口', '布局', '管理', '分屏'],
    order: 120,
  },

  disableSearchInput: true,
  mainView: () => WindowManagerView,
})
