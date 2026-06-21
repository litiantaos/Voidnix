// Tauri 命令名常量单一源（替换原 specta 生成的 bindings.ts）。
// 禁止裸 invoke('xxx')，统一走 CMD.xxx 通道；类型手写于 types/ 与各扩展。
// check:commands CI 对此文件与 Rust #[tauri::command] 集合作双向差集校验。
export const CMD = {
  // —— 窗口 / 应用（框架）——
  hideWindow: 'hide_window',
  isAppActive: 'is_app_active',
  getHomeDir: 'get_home_dir',
  pickDirectory: 'pick_directory',
  httpGet: 'http_get',
  openModuleSubview: 'open_module_subview',
  quitApp: 'quit_app',
  revealInFinder: 'reveal_in_finder',
  openExtensionsPrefs: 'open_extensions_prefs',
  openPrivacySettings: 'open_privacy_settings',

  // —— 权限（框架）——
  checkAccessibilityPermission: 'check_accessibility_permission',
  requestAccessibilityPermission: 'request_accessibility_permission',
  checkScreenRecordingPermission: 'check_screen_recording_permission',
  checkFullDiskAccessPermission: 'check_full_disk_access_permission',

  // —— 全局快捷键 / 录制（框架）——
  registerGlobalShortcut: 'register_global_shortcut',
  startShortcutRecording: 'start_shortcut_recording',
  stopShortcutRecording: 'stop_shortcut_recording',

  // —— clipboard ——
  getClipboardHistory: 'get_clipboard_history',
  pasteClipboardItem: 'paste_clipboard_item',
  pasteClipboardItems: 'paste_clipboard_items',
  deleteClipboardItems: 'delete_clipboard_items',
  toggleClipboardFavorite: 'toggle_clipboard_favorite',
  getClipboardImage: 'get_clipboard_image',
  clearClipboardHistory: 'clear_clipboard_history',
  pasteboardWriteText: 'pasteboard_write_text',
  setClipboardMaxDays: 'set_clipboard_max_days',

  // —— screenshot ——
  captureScreen: 'capture_screen',
  enterScreenshotMode: 'enter_screenshot_mode',
  exitScreenshotMode: 'exit_screenshot_mode',
  saveScreenshot: 'save_screenshot',
  copyScreenshotToClipboard: 'copy_screenshot_to_clipboard',
  ocrImage: 'ocr_image',
  detectTextRegions: 'detect_text_regions',
  screenshotOverlayReady: 'screenshot_overlay_ready',
  enterScrollCapture: 'enter_scroll_capture',
  exitScrollCapture: 'exit_scroll_capture',
  finishScrollCapture: 'finish_scroll_capture',
  saveScrollResult: 'save_scroll_result',
  copyScrollResultToClipboard: 'copy_scroll_result_to_clipboard',
  setScrollToolbarRect: 'set_scroll_toolbar_rect',
  pinImage: 'pin_image',
  pinGlobalMouse: 'pin_global_mouse',
  setPinWindowOpacity: 'set_pin_window_opacity',
  restorePinFocus: 'restore_pin_focus',
  getScreenInfo: 'get_screen_info',

  // —— awake ——
  setAwakeEnabled: 'set_awake_enabled',
  setAwakeDisplayMode: 'set_awake_display_mode',
  isAwakeEnabled: 'is_awake_enabled',

  // —— window-manager ——
  setFrontmostWindowLayout: 'set_frontmost_window_layout',
  setWindowManagerEnabled: 'set_window_manager_enabled',
  setSnapSize: 'set_snap_size',
  showSnapPanel: 'show_snap_panel',
  hideSnapPanel: 'hide_snap_panel',
  checkWindowManagerAccessibility: 'check_window_manager_accessibility',

  // —— finder-ext ——
  checkFinderExtAuthorized: 'check_finder_ext_authorized',
  setFinderExtEnabled: 'set_finder_ext_enabled',

  // —— zsh-autosuggestions ——
  setZshAutosuggestionsEnabled: 'set_zsh_autosuggestions_enabled',

  // —— translate ——
  getSelectedText: 'get_selected_text',
  getSelectedTextCached: 'get_selected_text_cached',
  translateAi: 'translate_ai',
  translateAiStream: 'translate_ai_stream',
  translateYoudao: 'translate_youdao',

  // —— agent ——
  agentRun: 'agent_run',
  agentApprove: 'agent_approve',
  agentAbort: 'agent_abort',

  // —— search ——
  searchApps: 'search_apps',
  searchFiles: 'search_files',
  launchApp: 'launch_app',
} as const
