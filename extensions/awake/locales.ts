import { registerMessages } from '@/runtime/i18n'

registerMessages({
  'awake.enable': { 'zh-CN': '启用唤醒', en: 'Enable Awake' },
  'awake.enableHint': {
    'zh-CN': '禁用系统睡眠，合盖熄屏与电池供电时仍可用，首次启动需验证授权',
    en: 'Disables system sleep, works with the lid closed and on battery. Administrator authorization once per launch.',
  },
  'awake.group.general': { 'zh-CN': '通用', en: 'General' },
  'awake.menubarToggle': { 'zh-CN': '显示菜单栏开关', en: 'Show Menu Bar Toggle' },
})
