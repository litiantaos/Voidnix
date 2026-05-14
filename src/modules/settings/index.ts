import { registerModule } from '@/core/module-registry'
import type { AppModule } from '@/types/module'
import SettingsView from './SettingsView.vue'

const mod: AppModule = {
  id: 'settings',
  name: '设置',
  description: '应用设置',
  icon: 'i-ri-settings-3-line',
  keywords: ['settings', 'config', '快捷键', '设置', '配置'],
  shortcut: '⌘,',
  order: 999, // 确保设置始终在最后
  layout: { view: SettingsView },
  onSearch: async () => {
    return []
  },
}

registerModule(mod)
