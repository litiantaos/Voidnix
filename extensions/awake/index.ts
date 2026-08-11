import { defineExtension } from '@/runtime/extension-registry'
import './locales'
import AwakeView from './View.vue'

export default defineExtension({
  meta: {
    id: 'awake',
    name: { 'zh-CN': '保持系统唤醒', en: 'Keep Awake' },
    description: {
      'zh-CN': '接入电源时允许合盖熄屏不休眠',
      en: 'Stay awake while plugged in (clamshell)',
    },
    icon: 'i-ri-macbook-line',
    keywords: ['awake', 'sleep', 'caffeine', '合盖', '休眠', '不休眠', '熄屏', '保持唤醒'],
    order: 160,
  },

  disableSearchInput: true,
  mainView: () => AwakeView,
})
