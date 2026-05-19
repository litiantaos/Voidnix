use std::str::FromStr;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Manager, Emitter};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

static SELECTED_TEXT: Mutex<String> = Mutex::new(String::new());
// app.hide() 后 window.is_visible() 在 release 构建里仍返回 true，
// 用 AtomicBool 自己跟踪窗口可见状态。
static WINDOW_VISIBLE: AtomicBool = AtomicBool::new(false);

#[tauri::command]
#[cfg_attr(feature = "specta", specta::specta)]
pub fn is_app_active() -> bool {
    #[cfg(target_os = "macos")]
    return crate::mac_utils::is_app_active();
    #[cfg(not(target_os = "macos"))]
    true
}

/// 供 webkit_tuning 模块读写窗口可见状态。
pub(crate) fn set_window_visible(v: bool) {
    WINDOW_VISIBLE.store(v, Ordering::SeqCst);
}

#[tauri::command]
pub fn hide_window(app: tauri::AppHandle) {
    // T13：替换为 webkit_tuning::hide_main，由驯化模块负责 alpha=0 + ignoresMouseEvents
    // 以及 click_monitor 移除，不再直接调用 window.hide() / app.hide()。
    crate::webkit_tuning::hide_main(&app);
    WINDOW_VISIBLE.store(false, Ordering::SeqCst);
}

#[tauri::command]
#[cfg_attr(feature = "specta", specta::specta)]
pub fn get_selected_text_cached() -> String {
    SELECTED_TEXT.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

// ============================================================================
// Shortcut registration
// ============================================================================

#[tauri::command]
pub async fn register_global_shortcut(
    app: tauri::AppHandle,
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

                        if id_for_check == "screenshot" {
                            // 截图必须在窗口显示/全屏之前完成，否则会截到自己
                            let app_clone = app_handle.clone();
                            std::thread::spawn(move || {
                                let result = crate::extensions::screenshot::capture_screen();
                                match result {
                                    Ok(data) => {
                                        // 直接在主线程进入截屏模式，绕开 main 窗口 IPC 中转。
                                        // enter_screenshot_mode_sync 内部会 SkyLight 迁移、
                                        // 揭开 alpha、向 screenshot webview 注入数据。
                                        let app_for_enter = app_clone.clone();
                                        let _ = app_clone.run_on_main_thread(move || {
                                            crate::extensions::screenshot::enter_screenshot_mode_sync(&app_for_enter, data);
                                        });
                                    }
                                    Err(e) => {
                                        let _ = app_clone.emit("screenshot-ready-error", e);
                                    }
                                }
                            });
                            return;
                        }

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
                                    crate::webkit_tuning::show_main(&app_handle);

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
                            crate::webkit_tuning::show_main(&app_handle);
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


pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new("shortcut")
        .build()
}
