import { registerMessages } from '@/runtime/i18n'

registerMessages({
  'awake.enable': { 'zh-CN': '启用唤醒', en: 'Enable Awake' },
  'awake.enableHint': {
    'zh-CN': '禁用系统睡眠，合盖熄屏与电池供电时仍可用，首次启动需验证授权',
    en: 'Disables system sleep, works with the lid closed and on battery. Administrator authorization once per launch.',
  },
  'awake.group.general': { 'zh-CN': '通用', en: 'General' },
  'awake.menubarToggle': { 'zh-CN': '在菜单栏图标菜单中显示', en: 'Show in Menu Bar Icon Menu' },
  'awake.screenPolicy': { 'zh-CN': '合盖熄屏方式', en: 'Closed-Lid Screen' },
  'awake.screenPolicyHint': {
    'zh-CN': '零亮度保持画面流活跃可远程控制，显示睡眠更省电但远程会冻结',
    en: 'Zero brightness keeps the framebuffer live for remote control; display sleep saves more power but freezes remote sessions.',
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
