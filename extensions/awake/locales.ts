import { registerMessages } from '@/runtime/i18n'

registerMessages({
  'awake.enable': { 'zh-CN': '启用唤醒', en: 'Enable Awake' },
  'awake.enableHint': {
    'zh-CN': '合盖、熄屏、用电池都不休眠；首次开启需授权',
    en: 'No sleep with lid closed or on battery; authorization needed the first time',
  },
  'awake.group.general': { 'zh-CN': '通用', en: 'General' },
  'awake.menubarToggle': { 'zh-CN': '在菜单栏中显示', en: 'Show in menu bar' },
  'awake.screenPolicy': { 'zh-CN': '合盖熄屏方式', en: 'Closed-Lid Screen' },
  'awake.screenPolicyHint': {
    'zh-CN': '零亮度可远程控制，显示睡眠更省电但远程会断',
    en: 'Zero brightness keeps remote control; display sleep saves power but freezes it',
  },
  'awake.screenPolicy.dim': {
    'zh-CN': '零亮度（远程可用）',
    en: 'Zero brightness (remote-friendly)',
  },
  'awake.screenPolicy.sleep': {
    'zh-CN': '显示睡眠（更省电）',
    en: 'Display sleep (saves power)',
  },
})
