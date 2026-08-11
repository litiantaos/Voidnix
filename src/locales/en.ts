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
