import { defineExtension } from '@/runtime/extension-registry'
import AwakeView from './View.vue'

export default defineExtension({
  meta: {
    id: 'awake',
    name: '保持系统唤醒',
    description: '合盖不休眠',
    icon: 'i-ri-macbook-line',
    keywords: ['awake', 'sleep', 'caffeine', '合盖', '休眠', '不休眠', '熄屏', '保持唤醒'],
    order: 50,
  },

  mainView: () => AwakeView,
  settingsView: () => AwakeView, // settings 枢纽浮出（mainView 兼任配置，复用同组件）
})
