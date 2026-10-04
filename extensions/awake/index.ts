import { listen } from '@tauri-apps/api/event'
import { defineExtension } from '@/runtime/extension-registry'
import { showToast } from '@/composables/useToast'
import './locales'
import AwakeView from './View.vue'

// 菜单栏开关/熄屏策略失败：Rust emit 错误文案，外部 toast 展示（主窗隐藏态
// 照常可见）——此前完全静默。模块级常驻监听（Rust 侧已过滤用户主动取消）
listen<string>('awake-menu-failed', (e) => {
  showToast(e.payload, { kind: 'error', duration: 4000 })
}).catch(() => {})

export default defineExtension({
  meta: {
    id: 'awake',
    name: { 'zh-CN': '保持系统唤醒', en: 'Keep Awake' },
    description: {
      'zh-CN': '禁用系统睡眠，不插电源也能合盖熄屏持续运行',
      en: 'Disable system sleep and keep running with the lid closed, battery included',
    },
    icon: 'i-ri-macbook-line',
    keywords: [
      'awake',
      'sleep',
      'caffeine',
      'disablesleep',
      '合盖',
      '休眠',
      '不休眠',
      '熄屏',
      '保持唤醒',
    ],
    order: 160,
  },

  disableSearchInput: true,
  mainView: () => AwakeView,
})
