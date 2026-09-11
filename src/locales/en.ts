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
  'welcome.fnScreenshot': { en: 'Screenshot' },
  'welcome.fnTranslate': { en: 'Translate' },
  'welcome.fnFinder': { en: 'Finder Tools' },
  'welcome.fnClipboard': { en: 'Clipboard' },
  'welcome.fnNotes': { en: 'Notes' },
  'welcome.tagline': { en: 'Think it, launch it' },
  'welcome.noteTools': { en: 'Type / to show extensions' },
  'welcome.noteSearch': { en: 'Type // for quick search' },
  'welcome.permScreenRecording': { en: 'Screen Recording' },
  'welcome.permAccessibility': { en: 'Accessibility' },
  'welcome.permFullDisk': { en: 'Full Disk' },
  'welcome.permGrant': { en: 'Grant' },
  'welcome.permGranted': { en: 'Granted' },
  'welcome.permUseScreenRecording': { en: 'Screenshot & window management' },
  'welcome.permUseAccessibility': { en: 'Translate selection · Finder hidden files' },
  'welcome.permUseFullDisk': { en: 'Save without folder prompts' },
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
  'settings.showWelcome': { en: 'Usage Guide' },
  'settings.autostart': { en: 'Launch at Login' },
  'settings.checkUpdate': { en: 'Check for Updates' },
  'settings.downloadAndInstall': { en: 'Download & Install' },
  'settings.installUpdate': { en: 'Install Update' },
  'settings.checking': { en: 'Checking…' },
  'settings.downloading': { en: 'Downloading…' },
  'settings.about': { en: 'About' },
  'settings.quit': { en: 'Quit' },
  'settings.group.app': { en: 'App' },
  'settings.group.privacy': { en: 'Privacy & Permissions' },
  'settings.privacy.screenRecording': { en: 'Screen Recording' },
  'settings.privacy.accessibility': { en: 'Accessibility' },
  'settings.privacy.fullDiskAccess': { en: 'Full Disk Access' },
  'settings.noResultsFound': { en: 'No matching settings' },
  'settings.quitConfirmTitle': { en: 'Quit Voidnix' },
  'settings.quitConfirmMessage': { en: 'Are you sure you want to quit Voidnix?' },
  'settings.quitLabel': { en: 'Quit' },
  'settings.upToDate': { en: 'Version v{version} is up to date.' },
  'settings.updateOK': { en: 'OK' },
  'settings.permChecking': { en: 'Checking…' },
  'settings.permGranted': { en: 'Granted' },
  'settings.permDenied': { en: 'Not granted — click to open System Settings' },
  'settings.clearInjections': { en: 'Clear Voidnix Injections' },
  'settings.clearInjectionsHint': {
    en: 'Remove injected blocks from .zshrc / .zprofile and the ai.env file',
  },
  'settings.clearInjectionsConfirmTitle': { en: 'Clear Voidnix Injections' },
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
  'updateDialog.newVersionFound': { en: 'New version available' },
  'updateDialog.installing': { en: 'Installing…' },
  'updateDialog.installNow': { en: 'Install & Restart' },

  // ─── search results ─────────────────────────
  'search.resultsLabel': { en: 'Search results' },
}
