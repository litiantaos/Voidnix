import { registerMessages } from '@/runtime/i18n'

registerMessages({
  'awake.enable': { 'zh-CN': '启用唤醒', en: 'Enable Awake' },
  'awake.enableHint': {
    'zh-CN': '通过虚拟外接显示器触发 Clamshell Mode，需接入电源',
    en: 'Trigger Clamshell Mode via a virtual external display. Requires power.',
  },
  'awake.group.display': { 'zh-CN': '显示器', en: 'Display' },
  'awake.displayMode': { 'zh-CN': '显示模式', en: 'Display Mode' },
  'awake.displayModeHint': {
    'zh-CN': '镜像与主屏显示相同画面，扩展提供独立桌面空间',
    en: 'Mirror shows the same as the main display; Extend provides an independent desktop space.',
  },
  'awake.mirror': { 'zh-CN': '镜像', en: 'Mirror' },
  'awake.extend': { 'zh-CN': '扩展', en: 'Extend' },
})
