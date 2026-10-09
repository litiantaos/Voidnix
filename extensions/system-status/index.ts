import { defineExtension } from '@/runtime/extension-registry'
import SystemStatusView from './View.vue'
import SystemStatusSettings from './Settings.vue'
import SystemStatusActions from './Actions.vue'
import './config'
import './locales'

export default defineExtension({
  meta: {
    id: 'system-status',
    name: { 'zh-CN': '系统状态', en: 'System Status' },
    description: { 'zh-CN': '实时查看电脑运行状态', en: 'See how your Mac is doing' },
    icon: 'i-ri-pulse-line',
    order: 135,
    keywords: [
      'system',
      'status',
      'cpu',
      'memory',
      'ram',
      'disk',
      'battery',
      'network',
      '系统',
      '状态',
      '性能',
      '硬件',
      '内存',
      '电池',
      '网络',
      '磁盘',
      '监控',
    ],
  },
  disableSearchInput: true,
  windowHeight: 'auto',
  mainView: () => SystemStatusView,
  searchBarAccessory: () => SystemStatusActions,
  /// config（设置）内容少走 auto 自适应
  subviews: { config: () => SystemStatusSettings },
  subviewHeights: { config: 'auto' },
  subviewTitle: {
    config: { 'zh-CN': '设置', en: 'Settings' },
  },
})
