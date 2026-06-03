import { registerModule } from '@/core/module-registry'
import { asyncView } from '@/core/async-view'
import type { AppModule } from '@/types/module'

const WindowManagerView = asyncView(() => import('./View.vue'))
const SnapPanelWindow = asyncView(() => import('./windows/SnapPanel.vue'))

const mod: AppModule = {
  id: 'window-manager',
  name: '窗口管理',
  description: '快速调整前台窗口位置和尺寸',
  icon: 'i-ri-layout-grid-line',
  keywords: ['window', 'manager', 'layout', 'snap', 'tile', '窗口', '布局', '管理', '分屏'],
  order: 10,
  view: WindowManagerView,
  windowViews: {
    'snap-panel': SnapPanelWindow,
  },
  onSearch: async () => [],
}

registerModule(mod)
