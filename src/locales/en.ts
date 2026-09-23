import type { Locale } from '@/runtime/i18n'

type LocaleMsg = Partial<Record<Locale, string>>
type Msgs = Record<string, LocaleMsg>

export const enMessages: Msgs = {
  // ─── common ──────────────────────────────────
  'common.cancel': { en: 'Cancel' },
  'common.confirm': { en: 'Confirm' },
  'common.copied': { en: 'Copied' },
  'common.close': { en: 'Close' },
  'common.save': { en: 'Save' },
  'common.delete': { en: 'Delete' },
  'common.loading': { en: 'Loading' },
  'common.goConfigure': { en: 'Configure' },
  // 拖拽指引浮窗两行说明（独立小窗悬浮于设置窗口底部外侧，三个权限授权会话通用；
  // \n 换行，图标高度与两行文本高度一致）
  'common.permDragHint': {
    en: 'Find Voidnix in the list above and turn on the switch,\nor drag the icon on the left into the list',
  },
  'common.noResults': { en: 'No results' },
  'common.enabled': { en: 'On' },
  'common.disabled': { en: 'Off' },
  'common.unknownError': { en: 'Unknown error' },
  'common.operationFailed': { en: 'Operation failed' },
  'common.networkError': { en: 'Network error, please try again later.' },
  'common.group.general': { en: 'General' },

  // ─── search ──────────────────────────────────
  'search.placeholder': { en: 'Search apps, files, extensions…' },
  'search.inExtension': { en: 'Search in {name}' },
  'search.searchIn': { en: 'Search {name}' },
  'search.newVersionHint': { en: 'New version available, click to view' },
  'search.browseToolsHint': { en: 'Type / to browse all tools' },

  // ─── welcome（首启引导）─────────────────────
  'welcome.isoModifier': { en: 'Modifier' },
  'welcome.isoAction': { en: 'Summon' },
  'welcome.fnAgent': { en: 'Agent' },
  'welcome.fnScreenshot': { en: 'Shot' },
  'welcome.fnTranslate': { en: 'Translate' },
  'welcome.fnFinder': { en: 'Finder Tools' },
  'welcome.fnClipboard': { en: 'Clipboard' },
  'welcome.fnNotes': { en: 'Notes' },
  'welcome.tagline': { en: 'Think it, launch it' },
  'welcome.noteTools': { en: 'Type / for extensions' },
  'welcome.noteSearch': { en: 'Type // for quick search' },
  'welcome.permScreenRecording': { en: 'Screen Recording' },
  'welcome.permAccessibility': { en: 'Device Control' },
  'welcome.permFullDisk': { en: 'Full Access' },
  'welcome.permGrant': { en: 'Grant' },
  'welcome.permGranted': { en: 'Granted' },
  'welcome.permUseScreenRecording': { en: 'Screenshot & window tools' },
  'welcome.permUseAccessibility': { en: 'Translate · hidden files' },
  'welcome.permUseFullDisk': { en: 'Search · save screenshots' },
  'welcome.navHint': { en: 'Enter / → next · ← back' },
  'welcome.navDone': { en: '← Back · Enter to start' },

  // ─── shortcut conflict（注册失败改键引导）─────
  'shortcut.conflictTitle': { en: 'Shortcut registration failed' },
  'shortcut.conflictBody': {
    en: 'These shortcuts may be taken by other apps (Raycast, input methods, etc.). Rebind them in Settings:',
  },
  'shortcut.conflictOpenSettings': { en: 'Open Settings' },
  'shortcut.conflictLater': { en: 'Later' },

  // ─── group titles ────────────────────────────
  'group.application': { en: 'Apps' },
  'group.file': { en: 'Files' },
  'group.extension': { en: 'Extensions' },
  'group.clipboard': { en: 'Clipboard' },
  'group.web': { en: 'Quick Actions' },

  // ─── settings ────────────────────────────────
  'settings.appearance': { en: 'Appearance' },
  'settings.appearance.auto': { en: 'Auto' },
  'settings.appearance.light': { en: 'Light' },
  'settings.appearance.dark': { en: 'Dark' },
  'settings.language': { en: 'Language' },
  'settings.language.zh-CN': { en: '中文' },
  'settings.language.en': { en: 'English' },
  'settings.shortcut': { en: 'Shortcut' },
  'settings.shortcutConflictHint': { en: 'Registration failed — may be taken by another app' },
  'settings.showWelcome': { en: 'Guide & Permissions' },
  'settings.autostart': { en: 'Launch at Login' },
  'settings.menubarIcon': { en: 'Menu Bar Icon' },
  'settings.checkUpdate': { en: 'Check for Updates' },
  'settings.downloadAndInstall': { en: 'Download & Install' },
  'settings.installUpdate': { en: 'Install Update' },
  'settings.checking': { en: 'Checking…' },
  'settings.downloading': { en: 'Downloading…' },
  'settings.about': { en: 'Open Source Repo' },
  'settings.website': { en: 'Website' },
  'settings.quit': { en: 'Quit' },
  'settings.group.general': { en: 'General' },
  'settings.group.about': { en: 'About' },
  'settings.group.advanced': { en: 'Advanced' },
  'settings.noResultsFound': { en: 'No matching settings' },
  'settings.quitConfirmTitle': { en: 'Quit Voidnix' },
  'settings.quitConfirmMessage': { en: 'Are you sure you want to quit Voidnix?' },
  'settings.quitLabel': { en: 'Quit' },
  'settings.upToDate': { en: 'Version v{version} is up to date.' },
  'settings.updateOK': { en: 'OK' },
  'settings.clearInjections': { en: 'Clear System Injections' },
  'settings.clearInjectionsHint': {
    en: 'Remove injected blocks from .zshrc / .zprofile and the ai.env file',
  },
  'settings.clearInjectionsConfirmTitle': { en: 'Clear System Injections' },
  'settings.clearInjectionsConfirmMessage': {
    en: `Removes all Voidnix blocks from \`~/.zshrc\` and \`~/.zprofile\` (AI credential source hook, zsh completions, etc.) and deletes \`~/.config/voidnix[/dev]/ai.env\` (contains plaintext API keys).

They will be re-created on demand if you keep using the features.`,
  },
  'settings.clearInjectionsOk': { en: 'Clear' },
  'settings.clearInjectionsDone': { en: 'Cleared {count} injection(s)' },
  'settings.clearInjectionsNone': { en: 'No Voidnix injections found' },
  'settings.clearInjectionsFailed': { en: 'Failed to clear injections' },

  // ─── action panel ────────────────────────────
  'action.openInFinder': { en: 'Reveal in Finder' },
  'action.copyPath': { en: 'Copy Path' },
  'action.copiedPath': { en: 'Path copied' },
  'action.size': { en: 'Size' },
  'action.version': { en: 'Version' },
  'action.created': { en: 'Created' },
  'action.modified': { en: 'Modified' },
  'action.lastOpened': { en: 'Last Opened' },
  'action.itemInfo': { en: 'Item Info' },

  // ─── web search ──────────────────────────────
  'web.search': { en: '{engine} Search' },
  'web.openLink': { en: 'Open URL' },
  'web.openInBrowser': { en: 'Open in default browser' },
  'web.openInBrowserBing': { en: 'Open in default browser (//b for Bing)' },

  // ─── markdown ────────────────────────────────
  'markdown.copyCode': { en: 'Copy code' },
  'markdown.copy': { en: 'Copy' },

  // ─── update dialog ──────────────────────────
  'updateDialog.later': { en: 'Later' },
  'updateDialog.checking': { en: 'Checking for updates…' },
  'updateDialog.upToDate': { en: 'Already up to date.' },
  'updateDialog.newVersionFound': { en: 'New version available' },
  'updateDialog.installing': { en: 'Installing…' },
  'updateDialog.installNow': { en: 'Install & Restart' },
  'updateDialog.retry': { en: 'Retry' },

  // ─── search results ─────────────────────────
  'search.resultsLabel': { en: 'Search results' },
}
