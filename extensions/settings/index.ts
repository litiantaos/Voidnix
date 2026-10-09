import { defineExtension } from '@/runtime/extension-registry'
import SettingsView from './View.vue'

export default defineExtension({
  meta: {
    id: 'settings',
    name: { 'zh-CN': '设置', en: 'Settings' },
    description: { 'zh-CN': '快捷键、外观等偏好设置', en: 'Shortcuts, appearance, and more' },
    icon: 'i-ri-settings-3-line',
    keywords: ['settings', 'config', '快捷键', '设置', '配置'],
    order: 998,
  },

  disableSearchInput: true,
  mainView: () => SettingsView,
})
