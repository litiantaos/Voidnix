/// 显示主窗口。
///
/// 编排：可见性状态 → 捕获原前台 PID → 平台 show（Space 迁移 + 前置）→
/// makeKey → click_monitor。NonactivatingPanel + LSUIElement 组合下显示不抢
/// NSApp active，原前台应用的菜单栏 / Dock 高亮全程不变。
pub fn show_main(app: &tauri::AppHandle) {
    use tauri::Manager;
    crate::runtime::shortcut::set_window_visible(true);

    #[cfg(target_os = "macos")]
    crate::platform::focus::capture_frontmost();

    if let Some(window) = app.get_webview_window("main") {
        #[cfg(target_os = "macos")]
        {
            crate::platform::skylight::move_webview_window_to_active_space(&window);
            crate::platform::window::bring_to_front(&window);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = window.show();
    }
    make_main_window_key(app);
    crate::platform::click_monitor::add(app);
}

/// 隐藏主窗口。
///
/// 编排：可见性状态 → 平台 hide（释放 key + orderOut）→ click_monitor 移除 →
/// restore_captured（deactivate + activate 原前台 app，归还 first responder）。
pub fn hide_main(app: &tauri::AppHandle) {
    use tauri::Manager;

    crate::runtime::shortcut::set_window_visible(false);

    #[cfg(target_os = "macos")]
    if let Some(window) = app.get_webview_window("main") {
        crate::platform::window::hide_native(&window);
    }
    #[cfg(not(target_os = "macos"))]
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    crate::platform::click_monitor::remove();

    // panel 偷走 system key 后,原 app 虽仍是 frontmost,但 first responder
    // 已丢失。restore_captured 先 deactivate 触发系统重新评估 key window,
    // 再 activate 原 app 完整恢复 first responder(光标回到原输入框)。
    #[cfg(target_os = "macos")]
    crate::platform::focus::restore_captured();
}

/// 将主窗口设为 key window。
pub fn make_main_window_key(app: &tauri::AppHandle) {
    use tauri::Manager;
    #[cfg(target_os = "macos")]
    if let Some(window) = app.get_webview_window("main") {
        crate::platform::window::make_key_window(&window);
    }
}

/// 主窗口框架级配置（panel 转换 + content 圆角，§2.8）。
/// 在 lib.rs setup 内 bootstrap 之后调用一次。
#[cfg(target_os = "macos")]
pub fn configure_main_window(app: &tauri::AppHandle) {
    use tauri::Manager;
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    crate::platform::window::apply_main_window_style(&window);
}

#[cfg(not(target_os = "macos"))]
pub fn configure_main_window(_app: &tauri::AppHandle) {}

/// 返回当前用户的 home 目录路径。
#[tauri::command]
pub fn get_home_dir() -> String {
    dirs::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| String::from("/tmp"))
}

/// 打开目录选择器（NSOpenPanel），作为独立浮窗运行，不附着主窗口。
/// 返回用户选择的目录路径，取消则返回空字符串。
#[tauri::command]
pub async fn pick_directory(app: tauri::AppHandle) -> Result<String, String> {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let app_clone = app.clone();
    app.run_on_main_thread(move || {
        #[cfg(target_os = "macos")]
        let path = crate::platform::window::pick_directory_modal(&app_clone);
        #[cfg(not(target_os = "macos"))]
        let path = String::new();
        let _ = tx.send(path);
    })
    .map_err(|e| e.to_string())?;

    // M-rs4：recv_timeout 兜底（NSOpenPanel 是模态对话框，理论秒级返回；
    // 用户长时间不操作时主线程闭包也不应永久阻塞 invoke）
    rx.recv_timeout(std::time::Duration::from_secs(60))
        .map_err(|e| e.to_string())
}
