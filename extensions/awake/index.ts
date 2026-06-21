import { defineExtension } from '@/runtime/extension-registry'
import AwakeView from './View.vue'

export default defineExtension({
  meta: {
    id: 'awake',
    name: '保持系统唤醒',
    description: '接入电源时允许合盖熄屏不休眠',
    icon: 'i-ri-macbook-line',
    keywords: ['awake', 'sleep', 'caffeine', '合盖', '休眠', '不休眠', '熄屏', '保持唤醒'],
    order: 50,
  },

  mainView: () => AwakeView,
})
