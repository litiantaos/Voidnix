import { defineExtension } from '@/runtime/extension-registry'
import WindowManagerView from './View.vue'

export default defineExtension({
  meta: {
    id: 'window-manager',
    name: '窗口管理',
    description: '窗口布局与分屏',
    icon: 'i-ri-layout-grid-line',
    keywords: ['window', 'manager', 'layout', 'snap', 'tile', '窗口', '布局', '管理', '分屏'],
    order: 120,
  },

  disableSearchInput: true,
  mainView: () => WindowManagerView,
})
