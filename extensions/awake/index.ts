import { registerModule } from '@/core/module-registry'
import { asyncView } from '@/core/async-view'
import type { AppModule } from '@/types/module'

const AwakeView = asyncView(() => import('./View.vue'))

const keywords = ['awake', 'sleep', 'caffeine', '合盖', '休眠', '不休眠', '熄屏', '保持唤醒']

const mod: AppModule = {
  id: 'awake',
  name: '保持系统唤醒',
  description: '合盖不休眠',
  icon: 'i-ri-macbook-line',
  keywords,
  order: 50,
  view: AwakeView,
  onSearch: async () => [],
}

registerModule(mod)
