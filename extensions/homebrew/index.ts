import { defineExtension } from '@/runtime/extension-registry'
import HomebrewView from './View.vue'
import DetailView from './DetailView.vue'
import './locales'

export default defineExtension({
  meta: {
    id: 'homebrew',
    name: 'Homebrew',
    description: { 'zh-CN': '包管理与一键更新升级', en: 'Package manager and one-click update' },
    icon: 'i-ri-cup-fill',
    keywords: [
      'brew',
      'homebrew',
      'bu',
      'update',
      'upgrade',
      'cleanup',
      'uninstall',
      'services',
      'package',
      '包管理',
      '更新',
      '升级',
      '清理',
      '卸载',
      '服务',
    ],
    order: 170,
  },

  placeholder: { 'zh-CN': '搜索包名', en: 'Search packages' },

  mainView: () => HomebrewView,
  subviews: {
    detail: () => DetailView,
  },
  subviewTitle: {
    detail: { 'zh-CN': '包详情', en: 'Package Details' },
  },
})
