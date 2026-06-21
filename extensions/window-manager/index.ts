import { defineExtension } from '@/runtime/extension-registry'
import { defineAsyncComponent } from 'vue'
import WindowManagerView from './View.vue'

// snap 面板独立窗口真按需：分屏拖拽时才加载
const SnapPanelWindow = defineAsyncComponent(() => import('./windows/SnapPanel.vue'))

export default defineExtension({
  meta: {
    id: 'window-manager',
    name: '窗口管理',
    description: '窗口布局与分屏',
    icon: 'i-ri-layout-grid-line',
    keywords: ['window', 'manager', 'layout', 'snap', 'tile', '窗口', '布局', '管理', '分屏'],
    order: 10,
  },

  mainView: () => WindowManagerView,
  settingsView: () => WindowManagerView, // settings 枢纽浮出（mainView 兼任配置，复用同组件）
  windowViews: {
    'snap-panel': () => SnapPanelWindow,
  },
})
