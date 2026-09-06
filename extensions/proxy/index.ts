import { defineExtension } from '@/runtime/extension-registry'
import './locales'
import ProxyView from './View.vue'
import ProxyActions from './Actions.vue'
import ProxySettings from './Settings.vue'
import ConnectionsView from './views/ConnectionsView.vue'
import RulesView from './views/RulesView.vue'
import LogsView from './views/LogsView.vue'

export default defineExtension({
  meta: {
    id: 'proxy',
    name: { 'zh-CN': '代理', en: 'Proxy' },
    description: { 'zh-CN': '基于 mihomo 的代理工具', en: 'Proxy tool based on mihomo' },
    icon: 'i-ri-signal-tower-line',
    keywords: ['proxy', '代理', 'mihomo', 'vpn', '节点', '订阅'],
    order: 40,
  },

  mainView: () => ProxyView,
  searchBarAccessory: () => ProxyActions,
  subviews: {
    config: () => ProxySettings,
    connections: () => ConnectionsView,
    rules: () => RulesView,
    logs: () => LogsView,
  },
  /// config（完全卸载）内容少走 auto 自适应；诊断三视图沿用固定 840
  subviewHeights: { config: 'auto' },
  subviewTitle: {
    config: { 'zh-CN': '设置', en: 'Settings' },
    connections: { 'zh-CN': '连接', en: 'Connections' },
    rules: { 'zh-CN': '规则', en: 'Rules' },
    logs: { 'zh-CN': '日志', en: 'Logs' },
  },
  windowHeight: 840,
})
