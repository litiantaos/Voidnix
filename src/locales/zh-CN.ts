import type { Locale } from '@/runtime/i18n'

type LocaleMsg = Partial<Record<Locale, string>>
type Msgs = Record<string, LocaleMsg>

export const zhCNMessages: Msgs = {
  // ─── common ──────────────────────────────────
  'common.cancel': { 'zh-CN': '取消' },
  'common.confirm': { 'zh-CN': '确定' },
  'common.copied': { 'zh-CN': '已复制' },
  'common.close': { 'zh-CN': '关闭' },
  'common.save': { 'zh-CN': '保存' },
  'common.delete': { 'zh-CN': '删除' },
  'common.loading': { 'zh-CN': '加载中' },
  'common.noResults': { 'zh-CN': '无结果' },
  'common.enabled': { 'zh-CN': '已开启' },
  'common.disabled': { 'zh-CN': '已关闭' },
  'common.unknownError': { 'zh-CN': '未知错误' },
  'common.operationFailed': { 'zh-CN': '操作失败' },
  'common.networkError': { 'zh-CN': '网络错误，请稍后重试。' },
  'common.group.general': { 'zh-CN': '通用' },

  // ─── search ──────────────────────────────────
  'search.placeholder': { 'zh-CN': '搜索应用、文件、扩展等' },
  'search.inExtension': { 'zh-CN': '在 {name} 中搜索' },
  'search.searchIn': { 'zh-CN': '搜索{name}' },
  'search.newVersionHint': { 'zh-CN': '发现新版本，点击查看' },

  // ─── group titles ────────────────────────────
  'group.application': { 'zh-CN': '应用' },
  'group.file': { 'zh-CN': '文件' },
  'group.extension': { 'zh-CN': '扩展' },
  'group.clipboard': { 'zh-CN': '剪贴板' },
  'group.web': { 'zh-CN': '快捷操作' },

  // ─── settings ────────────────────────────────
  'settings.appearance': { 'zh-CN': '外观' },
  'settings.appearance.auto': { 'zh-CN': '自动' },
  'settings.appearance.light': { 'zh-CN': '浅色' },
  'settings.appearance.dark': { 'zh-CN': '深色' },
  'settings.language': { 'zh-CN': '语言' },
  'settings.language.zh-CN': { 'zh-CN': '中文' },
  'settings.language.en': { 'zh-CN': 'English' },
  'settings.shortcut': { 'zh-CN': '启动快捷键' },
  'settings.autostart': { 'zh-CN': '开机自启' },
  'settings.checkUpdate': { 'zh-CN': '检查更新' },
  'settings.downloadAndInstall': { 'zh-CN': '下载并安装' },
  'settings.installUpdate': { 'zh-CN': '安装新版本' },
  'settings.checking': { 'zh-CN': '检查中…' },
  'settings.downloading': { 'zh-CN': '下载中…' },
  'settings.about': { 'zh-CN': '关于' },
  'settings.quit': { 'zh-CN': '退出应用' },
  'settings.group.app': { 'zh-CN': '应用' },
  'settings.group.privacy': { 'zh-CN': '隐私权限' },
  'settings.privacy.screenRecording': { 'zh-CN': '屏幕录制权限' },
  'settings.privacy.accessibility': { 'zh-CN': '辅助功能权限' },
  'settings.privacy.fullDiskAccess': { 'zh-CN': '完全磁盘访问权限' },
  'settings.noResultsFound': { 'zh-CN': '没有找到相关设置' },
  'settings.quitConfirmTitle': { 'zh-CN': '退出应用' },
  'settings.quitConfirmMessage': { 'zh-CN': '确定要退出 Voidnix 吗？' },
  'settings.quitLabel': { 'zh-CN': '退出' },
  'settings.upToDate': { 'zh-CN': '当前版本 v{version} 已是最新版本。' },
  'settings.updateOK': { 'zh-CN': '好的' },
  'settings.permChecking': { 'zh-CN': '检查中…' },
  'settings.permGranted': { 'zh-CN': '已授权' },
  'settings.permDenied': { 'zh-CN': '未授权 — 点击前往系统设置' },

  // ─── action panel ────────────────────────────
  'action.openInFinder': { 'zh-CN': '在访达中打开' },
  'action.copyPath': { 'zh-CN': '复制路径' },
  'action.copiedPath': { 'zh-CN': '已复制路径' },
  'action.size': { 'zh-CN': '大小' },
  'action.version': { 'zh-CN': '版本' },
  'action.created': { 'zh-CN': '创建时间' },
  'action.modified': { 'zh-CN': '修改时间' },
  'action.lastOpened': { 'zh-CN': '上次打开' },
  'action.itemInfo': { 'zh-CN': '项目信息' },

  // ─── web search ──────────────────────────────
  'web.search': { 'zh-CN': '{engine} 搜索' },
  'web.openLink': { 'zh-CN': '打开链接' },
  'web.openInBrowser': { 'zh-CN': '在默认浏览器中打开' },
  'web.openInBrowserBing': { 'zh-CN': '在默认浏览器中打开，//b 可使用 Bing 搜索' },

  // ─── markdown ────────────────────────────────
  'markdown.copyCode': { 'zh-CN': '复制代码' },
  'markdown.copy': { 'zh-CN': '复制' },

  // ─── update dialog ──────────────────────────
  'updateDialog.later': { 'zh-CN': '稍后' },
  'updateDialog.newVersionFound': { 'zh-CN': '发现新版本' },
  'updateDialog.installing': { 'zh-CN': '安装中…' },
  'updateDialog.installNow': { 'zh-CN': '立即安装并重启' },

  // ─── search results ─────────────────────────
  'search.resultsLabel': { 'zh-CN': '搜索结果' },
}
