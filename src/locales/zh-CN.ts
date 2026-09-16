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
  // 拖拽指引浮窗两行说明（独立小窗悬浮于设置窗口底部外侧，三个权限授权会话通用；
  // \n 换行，图标高度与两行文本高度一致）
  'common.permDragHint': {
    'zh-CN': '请在上方列表中找到 Voidnix 并打开开关，\n或拖拽左侧图标至列表中以完成授权',
  },
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
  'search.browseToolsHint': { 'zh-CN': '输入 / 浏览全部工具' },

  // ─── welcome（首启引导）─────────────────────
  'welcome.isoModifier': { 'zh-CN': '修饰键' },
  'welcome.isoAction': { 'zh-CN': '唤起窗口' },
  'welcome.fnAgent': { 'zh-CN': 'Agent' },
  'welcome.fnScreenshot': { 'zh-CN': '截屏' },
  'welcome.fnTranslate': { 'zh-CN': '翻译' },
  'welcome.fnFinder': { 'zh-CN': '访达工具' },
  'welcome.fnClipboard': { 'zh-CN': '剪贴板' },
  'welcome.fnNotes': { 'zh-CN': '记事本' },
  'welcome.tagline': { 'zh-CN': '想到就到，触手可及' },
  'welcome.noteTools': { 'zh-CN': '输入/显示扩展' },
  'welcome.noteSearch': { 'zh-CN': '输入//快速搜索' },
  'welcome.permScreenRecording': { 'zh-CN': '录屏' },
  'welcome.permAccessibility': { 'zh-CN': '设备控制' },
  'welcome.permFullDisk': { 'zh-CN': '完全访问' },
  'welcome.permGrant': { 'zh-CN': '授权' },
  'welcome.permGranted': { 'zh-CN': '已授权' },
  'welcome.permUseScreenRecording': { 'zh-CN': '截屏标注 · 窗口管理' },
  'welcome.permUseAccessibility': { 'zh-CN': '划词翻译 · 访达隐藏文件切换' },
  'welcome.permUseFullDisk': { 'zh-CN': '文件搜索 · 截图保存' },
  'welcome.navHint': { 'zh-CN': 'Enter / → 下一步 · ← 上一步' },
  'welcome.navDone': { 'zh-CN': '← 上一步 · Enter 开始使用' },

  // ─── shortcut conflict（注册失败改键引导）─────
  'shortcut.conflictTitle': { 'zh-CN': '快捷键注册失败' },
  'shortcut.conflictBody': {
    'zh-CN': '以下快捷键可能已被其它应用占用（如 Raycast、输入法等），请在设置中改为其它组合：',
  },
  'shortcut.conflictOpenSettings': { 'zh-CN': '打开设置' },
  'shortcut.conflictLater': { 'zh-CN': '稍后' },

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
  'settings.shortcutConflictHint': { 'zh-CN': '注册失败，可能已被其它应用占用' },
  'settings.showWelcome': { 'zh-CN': '使用引导' },
  'settings.autostart': { 'zh-CN': '开机自启' },
  'settings.menubarIcon': { 'zh-CN': '菜单栏图标' },
  'settings.checkUpdate': { 'zh-CN': '检查更新' },
  'settings.downloadAndInstall': { 'zh-CN': '下载并安装' },
  'settings.installUpdate': { 'zh-CN': '安装新版本' },
  'settings.checking': { 'zh-CN': '检查中…' },
  'settings.downloading': { 'zh-CN': '下载中…' },
  'settings.about': { 'zh-CN': '开源仓库' },
  'settings.website': { 'zh-CN': '官方网站' },
  'settings.quit': { 'zh-CN': '退出应用' },
  'settings.group.general': { 'zh-CN': '通用' },
  'settings.group.privacy': { 'zh-CN': '隐私权限' },
  'settings.group.about': { 'zh-CN': '关于' },
  'settings.group.advanced': { 'zh-CN': '高级' },
  'settings.privacy.screenRecording': { 'zh-CN': '录屏' },
  'settings.privacy.accessibility': { 'zh-CN': '设备控制' },
  'settings.privacy.fullDiskAccess': { 'zh-CN': '完全访问' },
  'settings.noResultsFound': { 'zh-CN': '没有找到相关设置' },
  'settings.quitConfirmTitle': { 'zh-CN': '退出应用' },
  'settings.quitConfirmMessage': { 'zh-CN': '确定要退出 Voidnix 吗？' },
  'settings.quitLabel': { 'zh-CN': '退出' },
  'settings.upToDate': { 'zh-CN': '当前版本 v{version} 已是最新版本。' },
  'settings.updateOK': { 'zh-CN': '好的' },
  'settings.permChecking': { 'zh-CN': '检查中…' },
  'settings.permGranted': { 'zh-CN': '已授权' },
  'settings.permDenied': { 'zh-CN': '未授权 — 点击前往系统设置' },
  'settings.clearInjections': { 'zh-CN': '清除系统注入' },
  'settings.clearInjectionsHint': {
    'zh-CN': '移除 .zshrc / .zprofile 注入块与 ai.env 凭证文件',
  },
  'settings.clearInjectionsConfirmTitle': { 'zh-CN': '清除系统注入' },
  'settings.clearInjectionsConfirmMessage': {
    'zh-CN': `将从 \`~/.zshrc\` 与 \`~/.zprofile\` 摘除全部 Voidnix 注入块（AI 凭证 source 钩子、zsh 补全等），并删除 \`~/.config/voidnix[/dev]/ai.env\`（含明文 API Key）。

继续使用相关功能时会按需重新写入。`,
  },
  'settings.clearInjectionsOk': { 'zh-CN': '清除' },
  'settings.clearInjectionsDone': { 'zh-CN': '已清除 {count} 处注入' },
  'settings.clearInjectionsNone': { 'zh-CN': '未发现 Voidnix 注入' },
  'settings.clearInjectionsFailed': { 'zh-CN': '清除注入失败' },

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
  'updateDialog.checking': { 'zh-CN': '正在检查更新…' },
  'updateDialog.upToDate': { 'zh-CN': '已是最新版本。' },
  'updateDialog.newVersionFound': { 'zh-CN': '发现新版本' },
  'updateDialog.installing': { 'zh-CN': '安装中…' },
  'updateDialog.installNow': { 'zh-CN': '立即安装并重启' },
  'updateDialog.retry': { 'zh-CN': '重试' },

  // ─── search results ─────────────────────────
  'search.resultsLabel': { 'zh-CN': '搜索结果' },
}
