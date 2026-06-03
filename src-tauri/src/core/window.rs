/// 显示主窗口。
pub fn show_main(app: &tauri::AppHandle) {
    use tauri::Manager;
    crate::core::shortcut::set_window_visible(true);
    if let Some(window) = app.get_webview_window("main") {
        #[cfg(target_os = "macos")]
        crate::macos::skylight::move_webview_window_to_active_space(&window);
        let _ = window.show();
    }
    make_main_window_key(app);
    crate::macos::click_monitor::add(app);
}

/// 隐藏主窗口。
pub fn hide_main(app: &tauri::AppHandle) {
    use tauri::Manager;
    crate::core::shortcut::set_window_visible(false);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    crate::macos::click_monitor::remove();
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

#[tauri::command]
pub fn set_main_window_size(
    app: tauri::AppHandle,
    width: f64,
    height: f64,
) -> Result<(), String> {
    use tauri::Manager;
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_size(tauri::LogicalSize::new(width, height));
        Ok(())
    } else {
        Err("Main window not found".into())
    }
}

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
                crate::macos::click_monitor::suppress(true);

                // 作为独立窗口运行（不附着父窗口，避免 sheet 遮罩）
                // NSModalResponseOK = 1
                let response: isize = objc2::msg_send![panel, runModal];

                crate::macos::click_monitor::suppress(false);
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


pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new("window").build()
}
