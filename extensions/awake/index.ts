import { defineExtension } from '@/runtime/extension-registry'
import './locales'
import AwakeView from './View.vue'

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
