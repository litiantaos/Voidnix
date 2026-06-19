import { defineExtension } from '@/runtime/extension-registry'
import { defineAsyncComponent } from 'vue'

const AwakeView = defineAsyncComponent(() => import('./View.vue'))

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
})
