use std::str::FromStr;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Manager, Emitter};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

static SELECTED_TEXT: Mutex<String> = Mutex::new(String::new());
// app.hide() 后 window.is_visible() 在 release 构建里仍返回 true，
// 用 AtomicBool 自己跟踪窗口可见状态。
static WINDOW_VISIBLE: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub fn is_app_active() -> bool {
    #[cfg(target_os = "macos")]
    return crate::mac_utils::is_app_active();
    #[cfg(not(target_os = "macos"))]
    true
}

#[tauri::command]
pub fn hide_window(app: AppHandle) {
    WINDOW_VISIBLE.store(false, Ordering::SeqCst);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
        #[cfg(all(target_os = "macos", not(debug_assertions)))]
        let _ = app.hide();
    }
    #[cfg(target_os = "macos")]
    click_monitor::remove();
}

#[tauri::command]
pub fn get_selected_text_cached() -> String {
    SELECTED_TEXT.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

// ============================================================================
// Native click-outside monitor (NSEvent global monitor)
// Works regardless of window focus state — the standard macOS approach
// for overlay/spotlight-style windows.
// ============================================================================

#[cfg(target_os = "macos")]
mod click_monitor {
    use std::sync::Mutex;
    use objc2::runtime::AnyObject;
    use tauri::{Emitter, Manager};

    struct SendObj(*mut AnyObject);
    unsafe impl Send for SendObj {}
    unsafe impl Sync for SendObj {}

    static MONITOR: Mutex<SendObj> = Mutex::new(SendObj(std::ptr::null_mut()));

    pub fn add(app: &tauri::AppHandle) {
        use objc2::ClassType;
        use objc2_app_kit::NSEvent;

        {
            let guard = MONITOR.lock().unwrap();
            if !guard.0.is_null() { return; }
        }

        let app_handle = app.clone();

        let block = block2::RcBlock::new(move |_event: *mut AnyObject| {
            unsafe {
                let app = match app_handle.get_webview_window("main") {
                    Some(w) => w,
                    None => return,
                };
                if !app.is_visible().unwrap_or(false) { return; }

                let loc: objc2_foundation::NSPoint = objc2::msg_send![NSEvent::class(), mouseLocation];
                let click_x = loc.x;
                let click_y_bottom = loc.y; // NSEvent.mouseLocation 已是屏幕坐标（左下原点）

                if let (Ok(pos), Ok(size)) = (app.outer_position(), app.outer_size()) {
                    let scale = app.scale_factor().unwrap_or(1.0);
                    let wx = pos.x as f64 / scale;
                    let wy = pos.y as f64 / scale;
                    let ww = size.width as f64 / scale;
                    let wh = size.height as f64 / scale;

                    let main_screen: *mut AnyObject =
                        objc2::msg_send![objc2::class!(NSScreen), mainScreen];
                    if main_screen.is_null() { return; }
                    let frame: objc2_foundation::NSRect =
                        objc2::msg_send![main_screen, frame];
                    let screen_h = frame.size.height;

                    // mouseLocation 是 macOS 屏幕坐标（左下原点），
                    // Tauri outer_position 是物理像素（左上原点），需转换
                    let click_y = screen_h - click_y_bottom;
                    let inside = click_x >= wx && click_x <= wx + ww
                        && click_y >= wy && click_y <= wy + wh;

                    if !inside {
                        let _ = app_handle.emit("click-outside", ());
                    }
                }
            }
        });

        unsafe {
            let mask = 1u64 << 1; // NSEventMaskLeftMouseDown
            let monitor: *mut AnyObject =
                objc2::msg_send![NSEvent::class(), addGlobalMonitorForEventsMatchingMask: mask, handler: &*block];
            if !monitor.is_null() {
                let _: () = objc2::msg_send![monitor, retain];
                let mut guard = MONITOR.lock().unwrap();
                *guard = SendObj(monitor);
                std::mem::forget(block);
            }
        }
    }

    pub fn remove() {
        use objc2::ClassType;
        use objc2_app_kit::NSEvent;

        let mut guard = MONITOR.lock().unwrap();
        let monitor = guard.0;
        if !monitor.is_null() {
            unsafe {
                let _: () = objc2::msg_send![NSEvent::class(), removeMonitor: monitor];
                let _: () = objc2::msg_send![monitor, release];
            }
            *guard = SendObj(std::ptr::null_mut());
        }
    }
}

// ============================================================================
// Shortcut registration
// ============================================================================

#[tauri::command]
pub async fn register_global_shortcut(
    app: AppHandle,
    id: String,
    new_shortcut: String,
    old_shortcut: Option<String>,
) -> Result<(), String> {
    let app_clone = app.clone();
    app.run_on_main_thread(move || {
        if let Some(old) = old_shortcut {
            if let Ok(old_sc) = Shortcut::from_str(&old) {
                let _ = app_clone.global_shortcut().unregister(old_sc);
            }
        }

        if new_shortcut.is_empty() {
            return;
        }

        if let Ok(new_sc) = Shortcut::from_str(&new_shortcut) {
            let shortcut_id = id.clone();
            let _ = app_clone.global_shortcut().on_shortcut(new_sc, move |app, _shortcut, event| {
                if event.state() == ShortcutState::Pressed {
                    // 按下瞬间的窗口可见性快照，前端用此判断 toggle 行为，
                    // 避免前端自维护的 isWindowVisible 与 Rust 端不同步
                    // （比如执行 onExecute 后 useSearchCommand 直接调 hide_window 时）。
                    let was_visible = WINDOW_VISIBLE.load(Ordering::SeqCst);
                    let _ = app.emit(
                        "shortcut-pressed",
                        serde_json::json!({
                            "id": shortcut_id.clone(),
                            "wasVisible": was_visible,
                        }),
                    );

                    let app_handle = app.clone();
                    let id_for_check = shortcut_id.clone();

                    let _ = app.run_on_main_thread(move || {
                        let window_hidden = !WINDOW_VISIBLE.load(Ordering::SeqCst);

                        // 在主线程获取前台 app PID（NSWorkspace 需要主线程）
                        #[cfg(target_os = "macos")]
                        let front_pid: Option<i32> = {
                            let ws = objc2_app_kit::NSWorkspace::sharedWorkspace();
                            ws.frontmostApplication().map(|a| a.processIdentifier() as i32)
                        };

                        crate::text_selection::log(&format!("[shortcut] id={} window_hidden={} front_pid={:?}", id_for_check, window_hidden, front_pid));

                        if id_for_check == "translate" {
                            #[cfg(target_os = "macos")]
                            {
                                if window_hidden {
                                    if let Ok(mut selected) = SELECTED_TEXT.lock() {
                                        *selected = String::new();
                                    }

                                    let self_pid = std::process::id() as i32;
                                    let target_pid = front_pid.filter(|&p| p != self_pid);

                                    crate::text_selection::log(&format!("[shortcut] translate triggered, front_pid={:?}, self_pid={}, target_pid={:?}", front_pid, self_pid, target_pid));

                                    // 主线程：先 AX（不阻塞），再 snapshot + inject Cmd+C
                                    // AX system-wide 不依赖 PID，在目标 app 失焦前后都能读
                                    let ax_text = crate::text_selection::try_ax();
                                    let snap = crate::text_selection::snapshot_clipboard();
                                    if ax_text.is_none() {
                                        if let Some(pid) = target_pid {
                                            crate::text_selection::inject_copy(pid);
                                        }
                                    }

                                    // 先 show 窗口（与其他快捷键行为一致），
                                    // 前端 shortcut-pressed 收到时 isWindowVisible 还是 false，
                                    // 能正常进入 waitForSelectedText 流程。
                                    let _ = app_handle.emit("showing-window", ());
                                    WINDOW_VISIBLE.store(true, Ordering::SeqCst);
                                    #[cfg(all(target_os = "macos", not(debug_assertions)))]
                                    let _ = app_handle.show();
                                    if let Some(window) = app_handle.get_webview_window("main") {
                                        let _ = window.show();
                                    }
                                    crate::mac_utils::activate_app();
                                    if let Some(window) = app_handle.get_webview_window("main") {
                                        let _ = window.set_focus();
                                    }
                                    #[cfg(target_os = "macos")]
                                    click_monitor::add(&app_handle);

                                    // 后台线程提取文本，完成后 emit translate-text-ready
                                    let app_clone = app_handle.clone();
                                    std::thread::spawn(move || {
                                        let text = if let Some(t) = ax_text {
                                            t
                                        } else {
                                            crate::text_selection::poll_clipboard(snap)
                                        };
                                        if let Ok(mut selected) = SELECTED_TEXT.lock() {
                                            *selected = text.clone();
                                        }
                                        let _ = app_clone.emit("translate-text-ready", text);
                                    });
                                    return;
                                } else {
                                    let _ = app_handle.emit("translate-text-ready", "");
                                }
                            }
                        }

                        // Show window after text extraction (translate) or immediately (others).
                        // For translate: extraction is fast (~5-30ms), so the delay is minimal.
                        if window_hidden {
                            let _ = app_handle.emit("showing-window", ());
                            // Release 构建会调用 app.hide() 隐藏整个应用，
                            // 必须先 show() 解除隐藏，再 activate，否则对隐藏中的 app
                            // 调用 activateIgnoringOtherApps 无效。
                            WINDOW_VISIBLE.store(true, Ordering::SeqCst);
                            #[cfg(all(target_os = "macos", not(debug_assertions)))]
                            let _ = app_handle.show();
                            if let Some(window) = app_handle.get_webview_window("main") {
                                let _ = window.show();
                            }
                            crate::mac_utils::activate_app();
                            if let Some(window) = app_handle.get_webview_window("main") {
                                let _ = window.set_focus();
                            }
                            #[cfg(target_os = "macos")]
                            click_monitor::add(&app_handle);
                        } else {
                            crate::mac_utils::activate_app();
                            if let Some(window) = app_handle.get_webview_window("main") {
                                let _ = window.set_focus();
                            }
                        }
                    });
                }
            });
        }
    }).map_err(|e| e.to_string())?;

    Ok(())
}
