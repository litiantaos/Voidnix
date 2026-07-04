import { defineExtension } from '@/runtime/extension-registry'
import ProxyView from './View.vue'
import ProxyActions from './Actions.vue'
import ConnectionsView from './views/ConnectionsView.vue'
import RulesView from './views/RulesView.vue'
import LogsView from './views/LogsView.vue'

export default defineExtension({
  meta: {
    id: 'proxy',
    name: '代理',
    description: '基于 mihomo 的代理工具',
    icon: 'i-ri-signal-tower-line',
    keywords: ['proxy', '代理', 'mihomo', 'vpn', '节点', '订阅'],
    order: 40,
  },

  mainView: () => ProxyView,
  searchBarAccessory: () => ProxyActions,
  subviews: {
    connections: () => ConnectionsView,
    rules: () => RulesView,
    logs: () => LogsView,
  },
  subviewTitle: {
    connections: '连接',
    rules: '规则',
    logs: '日志',
  },
  windowHeight: 840,
})
