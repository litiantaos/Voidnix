import { defineExtension } from '@/runtime/extension-registry'
import SettingsView from './View.vue'

export default defineExtension({
  meta: {
    id: 'settings',
    name: '设置',
    description: '应用设置',
    icon: 'i-ri-settings-3-line',
    keywords: ['settings', 'config', '快捷键', '设置', '配置'],
    order: 998,
  },

  mainView: () => SettingsView,
})
