import { defineAsyncComponent } from 'vue'
import { registerModule } from '@/core/module-registry'
import type { AppModule } from '@/types/module'

const AwakeView = defineAsyncComponent(() => import('./View.vue'))

const mod: AppModule = {
  id: 'awake',
  name: '保持系统唤醒',
  description: '通过虚拟外接显示器触发 macOS 原生的 Clamshell Mode，支持 MacBook 合盖熄屏不休眠',
  icon: 'i-ri-macbook-line',
  keywords: ['awake', 'sleep', 'caffeine', '合盖', '休眠', '不休眠', '熄屏', '保持唤醒'],
  order: 50,
  layout: { view: AwakeView },
  onSearch: async () => {
    return []
  },
}

registerModule(mod)