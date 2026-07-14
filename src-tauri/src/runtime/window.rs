/// 显示主窗口。
///
/// 编排：可见性状态 → 捕获原前台 PID → Space 迁移 + 前置 → makeKey →
/// click_monitor。NonactivatingPanel + LSUIElement 组合下显示不抢 NSApp
/// active，原前台应用的菜单栏 / 聚焦视图 / Dock 高亮全程不变。
///
/// 取舍：不 `activate_app`——抢 active 会让原 app resign（聚焦视图消失、
/// 界面态突变），打断感强于「面板下偶发 hover 穿透」。hide 时
/// `restore_captured` 交还 first responder。
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
    crate::platform::frontmost_watcher::add(app);
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
    crate::platform::frontmost_watcher::remove();

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

/// 主窗口框架级配置（panel 转换 + content 圆角）。
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

/// 一次 IPC 设置主窗口目标 frame，系统 animator 接管动画（NSAnimationContext）。
#[tauri::command]
pub fn set_main_frame(
    app: tauri::AppHandle,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    use tauri::Manager;
    let win = app
        .get_webview_window("main")
        .ok_or("main window not found")?;
    #[cfg(target_os = "macos")]
    crate::platform::window::animate_frame(&win, x, y, width, height);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = win.set_position(tauri::LogicalPosition::new(x, y));
        let _ = win.set_size(tauri::LogicalSize::new(width, height));
    }
    Ok(())
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

    // 用户操作时长不可控（大目录/找文件），不得短超时截断；
    // modal 关闭前阻塞 invoke 是预期行为，超时会丢选择且 modal 仍开着。
    rx.recv().map_err(|e| e.to_string())
}

/// 打开文件选择器（NSOpenPanel）。
/// `allowed_extensions`：扩展名列表（无点号）；空 = 不限制。
/// 取消返回空数组。
#[tauri::command]
pub async fn pick_files(
    app: tauri::AppHandle,
    allows_multiple: bool,
    allowed_extensions: Vec<String>,
) -> Result<Vec<String>, String> {
    let (tx, rx) = std::sync::mpsc::channel::<Vec<String>>();
    let app_clone = app.clone();
    app.run_on_main_thread(move || {
        #[cfg(target_os = "macos")]
        let paths = crate::platform::window::pick_files_modal(
            &app_clone,
            allows_multiple,
            allowed_extensions,
        );
        #[cfg(not(target_os = "macos"))]
        let paths = Vec::new();
        let _ = tx.send(paths);
    })
    .map_err(|e| e.to_string())?;

    rx.recv().map_err(|e| e.to_string())
}

/// 退出应用（设置页「退出」等）。
#[tauri::command]
pub fn quit_app(app_handle: tauri::AppHandle) {
    log::info!("Quitting app...");
    app_handle.exit(0);
}
