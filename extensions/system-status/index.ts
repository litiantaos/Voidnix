import { defineExtension } from '@/runtime/extension-registry'
import { makeToggleHandler } from '@/stores/app'
import SystemStatusView from './View.vue'

export default defineExtension({
  meta: {
    id: 'system-status',
    name: '系统状态',
    description: '硬件信息与系统实时状态',
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
  globalShortcuts: [
    {
      id: 'system-status',
      default: 'Alt+M',
      onExecute: makeToggleHandler('system-status'),
    },
  ],
})
