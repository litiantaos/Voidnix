/// 显示主窗口。
///
/// NonactivatingPanel + LSUIElement 组合下,通过 orderFrontRegardless +
/// makeKeyWindow 让面板取得键盘焦点,而不抢 NSApp active —— 原前台应用的
/// 菜单栏 / Dock 高亮全程不变,视觉上像浮层从未离开过当前应用。
pub fn show_main(app: &tauri::AppHandle) {
    use tauri::Manager;
    crate::runtime::shortcut::set_window_visible(true);

    #[cfg(target_os = "macos")]
    crate::platform::focus::capture_frontmost();

    if let Some(window) = app.get_webview_window("main") {
        #[cfg(target_os = "macos")]
        {
            crate::platform::skylight::move_webview_window_to_active_space(&window);
            use objc2_app_kit::NSWindow;
            if let Ok(raw) = window.ns_window() {
                unsafe {
                    let ns_window = raw.cast::<NSWindow>().as_ref().unwrap();
                    ns_window.orderFrontRegardless();
                }
            }
        }
        #[cfg(not(target_os = "macos"))]
        let _ = window.show();
    }
    make_main_window_key(app);
    crate::platform::click_monitor::add(app);
}

/// 隐藏主窗口。
///
/// 关闭面板后,显式 activate 原前台 app —— 它一直是 frontmost,
/// 菜单栏不会闪;但系统级 key window / first responder 需要主动还回去,
/// 否则用户得手动点一下输入框才能继续打字。
pub fn hide_main(app: &tauri::AppHandle) {
    use tauri::Manager;

    crate::runtime::shortcut::set_window_visible(false);

    #[cfg(target_os = "macos")]
    if let Some(window) = app.get_webview_window("main") {
        use objc2_app_kit::NSWindow;
        if let Ok(raw) = window.ns_window() {
            unsafe {
                let ns_window = raw.cast::<NSWindow>().as_ref().unwrap();
                ns_window.resignKeyWindow();
                ns_window.orderOut(None);
            }
        }
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
        use objc2_app_kit::NSWindow;
        let raw = window.ns_window().unwrap().cast::<NSWindow>();
        let ns_window = unsafe { raw.as_ref().unwrap() };
        ns_window.makeKeyWindow();
    }
}

/// 主窗口框架级配置：content view 圆角 + panel 转换（§2.8）。
/// 在 lib.rs setup 内 bootstrap 之后调用一次。
#[cfg(target_os = "macos")]
pub fn configure_main_window(app: &tauri::AppHandle) {
    use tauri::Manager;
    use objc2_app_kit::NSWindow;
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let raw = window.ns_window().unwrap().cast::<NSWindow>();
    let ns_window = unsafe { raw.as_ref().unwrap() };
    if let Some(content_view) = ns_window.contentView() {
        let _: () = unsafe { objc2::msg_send![&content_view, setWantsLayer: true] };
        let layer: *mut objc2::runtime::AnyObject =
            unsafe { objc2::msg_send![&content_view, layer] };
        if !layer.is_null() {
            let _: () = unsafe { objc2::msg_send![layer, setCornerRadius: 16.0_f64] };
            let _: () = unsafe { objc2::msg_send![layer, setMasksToBounds: true] };
        }
    }
    crate::platform::panel::convert_to_panel(raw.cast());
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
        {
            use objc2::runtime::AnyObject;
            use objc2_foundation::NSString;
            unsafe {
                let panel_cls = objc2::class!(NSOpenPanel);
                let panel: *mut AnyObject = objc2::msg_send![panel_cls, openPanel];

                // 仅允许选择目录
                let _: () = objc2::msg_send![panel, setCanChooseFiles: false];
                let _: () = objc2::msg_send![panel, setCanChooseDirectories: true];
                let _: () = objc2::msg_send![panel, setAllowsMultipleSelection: false];
                let _: () = objc2::msg_send![panel, setCanCreateDirectories: true];

                // panel 运行期间暂停 click-outside 检测，防止点击 panel 触发窗口隐藏
                crate::platform::click_monitor::suppress(true);

                // 作为独立窗口运行（不附着父窗口，避免 sheet 遮罩）
                // NSModalResponseOK = 1
                let response: isize = objc2::msg_send![panel, runModal];

                crate::platform::click_monitor::suppress(false);
                make_main_window_key(&app_clone);

                if response == 1 {
                    let urls: *mut AnyObject = objc2::msg_send![panel, URLs];
                    let count: usize = objc2::msg_send![urls, count];
                    if count > 0 {
                        let url: *mut AnyObject = objc2::msg_send![urls, objectAtIndex: 0usize];
                        let path: *mut NSString = objc2::msg_send![url, path];
                        let s = (*path).to_string();
                        let _ = tx.send(s);
                        return;
                    }
                }
                let _ = tx.send(String::new());
            }
        }
        #[cfg(not(target_os = "macos"))]
        { let _ = tx.send(String::new()); }
    }).map_err(|e| e.to_string())?;

    rx.recv().map_err(|e| e.to_string())
}
