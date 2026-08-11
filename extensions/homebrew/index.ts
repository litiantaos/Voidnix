import { defineExtension } from '@/runtime/extension-registry'
import HomebrewView from './View.vue'
import DetailView from './DetailView.vue'

export default defineExtension({
  meta: {
    id: 'homebrew',
    name: 'Homebrew',
    description: '包管理与一键更新升级',
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

  placeholder: '搜索包名',

  mainView: () => HomebrewView,
  subviews: {
    detail: () => DetailView,
  },
  subviewTitle: {
    detail: '包详情',
  },
})
