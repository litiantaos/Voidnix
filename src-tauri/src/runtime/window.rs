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
            // 1) Space Add  2) present（定位+alpha=1+orderFront+capture_mouse） 3) 再 Add
            // present_on_cursor_screen 已完整覆盖 level/alpha/orderFront/event shape，
            // 无需再调 bring_to_front（历史遗留冗余，每次 show 多 3 次 SkyLight mach_msg IPC）。
            // 禁止在 present 之后调 Tauri show()——会按内部缓存位置把窗打回主屏
            crate::platform::skylight::add_webview_to_all_active_spaces(&window);
            crate::platform::window::present_on_cursor_screen(&window);
            crate::platform::skylight::add_webview_to_all_active_spaces(&window);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = window.show();
    }
    make_main_window_key(app);
    crate::platform::click_monitor::add(app);
    crate::platform::frontmost_watcher::add(app);
}

/// 显示主窗口（前端主动调用）。
///
/// 扩展快捷键从隐藏呼出时，前端先 `setActiveExtension` 再调本命令 show，
/// 确保窗口渲染第一帧时已是目标扩展视图（避免先渲染旧视图再切换的闪现）。
/// `main` 快捷键无需切换视图，仍由 shortcut.rs callback 自动 show。
#[tauri::command]
pub fn show_window(app: tauri::AppHandle) {
    // show_main 操作 NSWindow（cocoa API 必须 main thread），与 hide_window 同路径
    let _ = app.clone().run_on_main_thread(move || {
        show_main(&app);
    });
}

/// 隐藏主窗口。
///
/// 编排：可见性状态 → 平台 hide（resignKey + alpha=0 + ignoresMouse，**不** orderOut）→
/// click_monitor 移除 → restore_captured（deactivate + activate 原前台 app，归还 first responder）。
pub fn hide_main(app: &tauri::AppHandle) {
    use tauri::Manager;

    crate::runtime::shortcut::set_window_visible(false);

    #[cfg(target_os = "macos")]
    if let Some(window) = app.get_webview_window("main") {
        crate::platform::window::cancel_pending_present();
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

/// 设置所有窗口外观（appearance）：`auto` 跟随系统 / `light` / `dark`。
/// 主题切换时由前端 theme.ts 调用，对 main 及所有已存在的窗口统一应用。
#[tauri::command]
pub fn set_window_appearance(app: tauri::AppHandle, mode: String) {
    use tauri::{Emitter, Manager};
    let mode_for_emit = mode.clone();
    // run_on_main_thread 借用 self 但闭包 move 了 app_for_windows，
    // 故在 clone 上调用 + 再 clone 传入闭包（AppHandle clone 开销低）。
    let app_for_windows = app.clone();
    let _ = app_for_windows.clone().run_on_main_thread(move || {
        for (_, window) in app_for_windows.webview_windows() {
            #[cfg(target_os = "macos")]
            crate::platform::window::apply_window_appearance(&window, &mode);
            #[cfg(not(target_os = "macos"))]
            let _ = (&window, &mode);
        }
    });
    // 广播主题变更：invisible 子窗口（screenshot/snap-panel）的 setAppearance 不会触发
    // WKWebView matchMedia change 事件，pin 窗口根本没有 setAppearance。
    // 统一靠事件通知所有子窗口前端更新 DOM data-theme。
    let _ = app.emit("appearance-changed", &mode_for_emit);
}

/// 返回缓存的 appearance mode（auto/light/dark）。子窗口前端读取以对齐 main 的强制主题，
/// 替代不可靠的 matchMedia（子窗口未设 setAppearance，prefers-color-scheme 跟随系统）。
#[tauri::command]
pub fn get_cached_appearance() -> Option<String> {
    crate::platform::window::cached_appearance()
}

/// 退出应用（设置页「退出」等）。
#[tauri::command]
pub fn quit_app(app_handle: tauri::AppHandle) {
    log::info!("Quitting app...");
    app_handle.exit(0);
}
