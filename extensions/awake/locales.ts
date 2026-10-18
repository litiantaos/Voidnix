import { registerMessages } from '@/runtime/i18n'

registerMessages({
  'awake.enable': { 'zh-CN': '开启系统唤醒', en: 'Enable Keep Awake' },
  'awake.enableHint': {
    'zh-CN': '只控制系统是否休眠，不决定屏幕亮/熄；首次开启需授权',
    en: 'Only controls system sleep, not the display; authorization needed the first time',
  },
  'awake.group.system': { 'zh-CN': '系统', en: 'System' },
  'awake.group.screen': { 'zh-CN': '屏幕', en: 'Screen' },
  'awake.menubarToggle': { 'zh-CN': '在菜单栏中显示', en: 'Show in menu bar' },
  'awake.screenOpen': { 'zh-CN': '开盖时屏幕', en: 'Screen When Lid Open' },
  'awake.screenOpenHint': {
    'zh-CN': '跟随系统由 macOS 管理；零亮度可远控，长按左键 2 秒退出',
    en: 'Follows macOS; Zero Brightness keeps remote capture available — hold left mouse for 2s to exit',
  },
  'awake.screenOpen.auto': { 'zh-CN': '跟随系统', en: 'Follow System' },
  'awake.screenOpen.zero': {
    'zh-CN': '零亮度（隐私/远控）',
    en: 'Zero Brightness (Privacy/Remote)',
  },
  'awake.screenClosed': { 'zh-CN': '合盖时屏幕', en: 'Screen When Lid Closed' },
  'awake.screenClosedHint': {
    'zh-CN': '显示睡眠更省电，但远控会断开；零亮度保持远控可用',
    en: 'Display Sleep saves power but drops the remote; Zero Brightness keeps it available',
  },
  'awake.screenClosed.sleep': { 'zh-CN': '显示睡眠（更省电）', en: 'Display Sleep (Saves Power)' },
  'awake.screenClosed.dim': {
    'zh-CN': '零亮度（远程可用）',
    en: 'Zero Brightness (Remote-Friendly)',
  },
  'awake.privacyNeedsAwake': {
    'zh-CN': '请先开启「系统唤醒」；开盖零亮度依赖它保持远控',
    en: 'Enable “Keep Awake” first; the lid-open zero-brightness mode depends on it for remote capture',
  },
})
