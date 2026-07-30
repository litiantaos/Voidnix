// Tauri 命令名常量单一源（替换原 specta 生成的 bindings.ts）。
// 禁止裸 invoke('xxx')，统一走 CMD.xxx 通道；类型手写于 types/ 与各扩展。
// check:commands CI 对此文件与 Rust #[tauri::command] 集合作双向差集校验。
export const CMD = {
  // —— 窗口 / 应用（框架）——
  hideWindow: 'hide_window',
  showWindow: 'show_window',
  isAppActive: 'is_app_active',
  getHomeDir: 'get_home_dir',
  pickDirectory: 'pick_directory',
  pickFiles: 'pick_files',
  setMainFrame: 'set_main_frame',
  setWindowAppearance: 'set_window_appearance',
  getCachedAppearance: 'get_cached_appearance',
  httpGet: 'http_get',
  openExtensionSubview: 'open_extension_subview',
  quitApp: 'quit_app',
  revealInFinder: 'reveal_in_finder',
  openPrivacySettings: 'open_privacy_settings',

  // —— 开机自启（SMAppService Login Item，macOS 13+）——
  isAutostartEnabled: 'is_autostart_enabled',
  enableAutostart: 'enable_autostart',
  disableAutostart: 'disable_autostart',

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
  getClipboardText: 'get_clipboard_text',
  updateClipboardText: 'update_clipboard_text',
  clearClipboardHistory: 'clear_clipboard_history',
  pasteboardWriteText: 'pasteboard_write_text',
  pasteboardPasteText: 'pasteboard_paste_text',
  setClipboardMaxDays: 'set_clipboard_max_days',

  // —— 语音朗读（框架，say CLI）——
  speakText: 'speak_text',
  stopSpeech: 'stop_speech',

  // —— screenshot ——
  // capture_screen / enter_screenshot_mode 仅 Rust 内部（快捷键路径），不暴露 IPC
  exitScreenshotMode: 'exit_screenshot_mode',
  saveScreenshot: 'save_screenshot',
  copyScreenshotToClipboard: 'copy_screenshot_to_clipboard',
  ocrImage: 'ocr_image',
  detectTextRegions: 'detect_text_regions',
  screenshotOverlayReady: 'screenshot_overlay_ready',
  readPickerImage: 'read_picker_image',
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

  // —— awake ——
  setAwakeEnabled: 'set_awake_enabled',
  setAwakeDisplayMode: 'set_awake_display_mode',
  isAwakeEnabled: 'is_awake_enabled',

  // —— clean-mode ——
  setCleanModeEnabled: 'set_clean_mode_enabled',
  isCleanModeEnabled: 'is_clean_mode_enabled',

  // —— window-manager ——
  setFrontmostWindowLayout: 'set_frontmost_window_layout',
  setWindowManagerEnabled: 'set_window_manager_enabled',
  setSnapSize: 'set_snap_size',
  showSnapPanel: 'show_snap_panel',
  hideSnapPanel: 'hide_snap_panel',

  // —— finder-ext ——
  finderRunAction: 'finder_run_action',
  finderSelectedPaths: 'finder_selected_paths',

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
  agentAbort: 'agent_abort',

  // —— ai-providers ——
  aiProvidersExport: 'ai_providers_export',
  aiProvidersExportDir: 'ai_providers_export_dir',
  aiProvidersEnvSnapshot: 'ai_providers_env_snapshot',
  aiProvidersZhipuQuota: 'ai_providers_zhipu_quota',
  aiProvidersDeepseekBalance: 'ai_providers_deepseek_balance',

  // —— search ——
  searchApps: 'search_apps',
  getAppIcons: 'get_app_icons',
  searchFiles: 'search_files',
  launchApp: 'launch_app',
  getPathMetadata: 'get_path_metadata',

  // —— proxy ——
  setProxyEnabled: 'set_proxy_enabled',
  isProxyEnabled: 'is_proxy_enabled',
  proxyCoreStatus: 'proxy_core_status',
  proxyEnsureCore: 'proxy_ensure_core',
  proxyCheckUpdate: 'proxy_check_update',
  proxyUpdateCore: 'proxy_update_core',
  proxyUpdateSubscription: 'proxy_update_subscription',
  proxyRemoveSubscription: 'proxy_remove_subscription',
  proxyGetProxies: 'proxy_get_proxies',
  proxySelectProxy: 'proxy_select_proxy',
  proxyTestGroupDelayStream: 'proxy_test_group_delay_stream',
  proxySetMode: 'proxy_set_mode',
  proxyReconnect: 'proxy_reconnect',
  proxyGetRules: 'proxy_get_rules',
  proxyTrafficStream: 'proxy_traffic_stream',
  proxyConnectionsStream: 'proxy_connections_stream',
  proxyLogsStream: 'proxy_logs_stream',
  proxyStopStream: 'proxy_stop_stream',

  // —— system-status ——
  systemStaticInfo: 'system_static_info',
  systemSnapshot: 'system_snapshot',

  // —— video ——
  videoCoreStatus: 'video_core_status',
  videoEnsureCore: 'video_ensure_core',
  videoProbe: 'video_probe',
  videoRun: 'video_run',
  videoCancel: 'video_cancel',
  videoJobStatus: 'video_job_status',

  // —— image ——
  imageRemoveBg: 'image_remove_bg',
  imageStitch: 'image_stitch',
  imageReadPreview: 'image_read_preview',
  imageSaveResult: 'image_save_result',
  imageCopyToClipboard: 'image_copy_to_clipboard',
} as const
