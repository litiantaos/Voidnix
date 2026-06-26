import { defineExtension } from '@/runtime/extension-registry'
import ProxyView from './View.vue'

export default defineExtension({
  meta: {
    id: 'proxy',
    name: '代理',
    description: '基于 mihomo 的代理客户端',
    icon: 'i-ri-signal-tower-line',
    keywords: ['proxy', '代理', 'mihomo', 'vpn', '节点', '订阅'],
    order: 7,
  },

  disableSearchInput: true,
  mainView: () => ProxyView,
  placeholder: '代理',
  windowHeight: 560,
})
