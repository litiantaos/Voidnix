import { registerModule } from '@/core/module-registry'
import { asyncView } from '@/core/async-view'
import type { AppModule, ModuleSearchItem } from '@/types/module'

const SettingsView = asyncView(() => import('./View.vue'))

const keywords = ['settings', 'config', '快捷键', '设置', '配置']

const SETTINGS_SEARCH_ITEMS: ModuleSearchItem[] = [
  {
    id: 'app-shortcut',
    title: '启动快捷键',
    icon: 'i-ri-keyboard-line',
    keywords: ['应用', 'app', '快捷键', 'shortcut', 'keyboard', '唤醒'],
    group: '应用',
  },
  {
    id: 'check-update',
    title: '检查更新',
    icon: 'i-ri-refresh-line',
    keywords: ['更新', 'update', '版本', 'version', '检查', '应用', 'app'],
    group: '应用',
  },
  {
    id: 'about',
    title: '关于',
    icon: 'i-ri-information-line',
    keywords: ['关于', 'about', 'github', '项目', 'project'],
    group: '应用',
  },
  {
    id: 'quit-app',
    title: '退出应用',
    icon: 'i-ri-logout-box-line',
    keywords: ['退出', 'quit', 'exit', '关闭', 'close'],
    group: '应用',
  },
  {
    id: 'perm-screen-recording',
    title: '屏幕录制权限',
    icon: 'i-ri-alert-line',
    keywords: [
      '权限',
      '隐私',
      '录制',
      '辅助',
      '磁盘',
      'accessibility',
      'screen',
      'disk',
      'privacy',
    ],
    group: '隐私权限',
  },
  {
    id: 'perm-accessibility',
    title: '辅助功能权限',
    icon: 'i-ri-alert-line',
    keywords: [
      '权限',
      '隐私',
      '录制',
      '辅助',
      '磁盘',
      'accessibility',
      'screen',
      'disk',
      'privacy',
    ],
    group: '隐私权限',
  },
  {
    id: 'perm-full-disk-access',
    title: '完全磁盘访问权限',
    icon: 'i-ri-alert-line',
    keywords: [
      '权限',
      '隐私',
      '录制',
      '辅助',
      '磁盘',
      'accessibility',
      'screen',
      'disk',
      'privacy',
    ],
    group: '隐私权限',
  },
]

const mod: AppModule = {
  id: 'settings',
  name: '设置',
  description: '应用设置',
  icon: 'i-ri-settings-3-line',
  keywords,
  shortcut: '⌘,',
  order: 999,
  view: SettingsView,
  onSearch: async () => [],
  searchItems: () => SETTINGS_SEARCH_ITEMS,
}

export { mod, SETTINGS_SEARCH_ITEMS }
registerModule(mod)
