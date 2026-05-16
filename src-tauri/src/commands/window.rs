// 主窗口尺寸调整命令。转发到 webkit_tuning::resize_main。
// T11 实装：接入 webkit_tuning 顶层入口。

#[tauri::command]
pub fn set_main_window_size(
    app: tauri::AppHandle,
    width: f64,
    height: f64,
) -> Result<(), String> {
    crate::webkit_tuning::resize_main(&app, width, height)
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
            use tauri::Manager;
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
                crate::commands::shortcut::suppress_click_monitor(true);

                // 作为独立窗口运行（不附着父窗口，避免 sheet 遮罩）
                // NSModalResponseOK = 1
                let response: isize = objc2::msg_send![panel, runModal];

                // panel 关闭后恢复检测，重新激活 Voidnix 并抢回主窗口焦点
                crate::commands::shortcut::suppress_click_monitor(false);
                crate::mac_utils::activate_app();
                if let Some(win) = app_clone.get_webview_window("main") {
                    let _ = win.set_focus();
                }

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
