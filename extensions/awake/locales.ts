import { registerMessages } from '@/runtime/i18n'

registerMessages({
  'awake.enable': { 'zh-CN': '启用合盖不休眠', en: 'Enable lid-closed keep-awake' },
  'awake.enableHint': {
    'zh-CN': '禁用系统睡眠，合盖熄屏仍持续运行，含电池供电。需管理员授权，启动后首次开启验证一次',
    en: 'Disables system sleep so the Mac keeps running with the lid shut and screen off, on battery too. Administrator authorization is requested once per launch.',
  },
  'awake.group.sleep': { 'zh-CN': '睡眠', en: 'Sleep' },
})
