import { defineExtension } from '@/runtime/extension-registry'
import SystemStatusView from './View.vue'
import './locales'

export default defineExtension({
  meta: {
    id: 'system-status',
    name: { 'zh-CN': '系统状态', en: 'System Status' },
    description: {
      'zh-CN': '硬件信息与系统实时状态',
      en: 'Hardware info and real-time system status',
    },
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
})
