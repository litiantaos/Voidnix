import { defineExtension } from '@/runtime/extension-registry'
import './locales'
import WindowManagerView from './View.vue'

export default defineExtension({
  meta: {
    id: 'window-manager',
    name: { 'zh-CN': '窗口管理', en: 'Window Manager' },
    description: { 'zh-CN': '拖动窗口，自动分屏', en: 'Drag a window to snap it' },
    icon: 'i-ri-layout-grid-line',
    keywords: ['window', 'manager', 'layout', 'snap', 'tile', '窗口', '布局', '管理', '分屏'],
    order: 120,
  },

  disableSearchInput: true,
  mainView: () => WindowManagerView,
})
