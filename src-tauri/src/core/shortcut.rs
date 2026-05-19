use std::str::FromStr;
use std::sync::{LazyLock, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Manager, Emitter};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

pub(crate) static SELECTED_TEXT: Mutex<String> = Mutex::new(String::new());
// app.hide() 后 window.is_visible() 在 release 构建里仍返回 true，
// 用 AtomicBool 自己跟踪窗口可见状态。
static WINDOW_VISIBLE: AtomicBool = AtomicBool::new(false);

// ── 扩展钩子注册 ─────────────────────────────────────────────────────────────

pub struct ShortcutContext {
    pub window_hidden: bool,
    #[cfg(target_os = "macos")]
    pub front_pid: Option<i32>,
}

type ShortcutHook = Box<dyn Fn(&tauri::AppHandle, &ShortcutContext) -> bool + Send + Sync>;

static SHORTCUT_HOOKS: LazyLock<Mutex<std::collections::HashMap<String, ShortcutHook>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

pub fn register_shortcut_hook(id: &str, hook: ShortcutHook) {
    SHORTCUT_HOOKS.lock().unwrap().insert(id.to_string(), hook);
}

#[tauri::command]
#[cfg_attr(feature = "specta", specta::specta)]
pub fn is_app_active() -> bool {
    #[cfg(target_os = "macos")]
    return crate::macos::mac_utils::is_app_active();
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
    crate::macos::webkit_tuning::hide_main(&app);
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

                        #[cfg(target_os = "macos")]
                        let front_pid: Option<i32> = {
                            let ws = objc2_app_kit::NSWorkspace::sharedWorkspace();
                            ws.frontmostApplication().map(|a| a.processIdentifier() as i32)
                        };
                        #[cfg(not(target_os = "macos"))]
                        let front_pid: Option<i32> = None;

                        let ctx = ShortcutContext { window_hidden, front_pid };

                        // ── 检查扩展注册的钩子 ──
                        if let Ok(hooks) = SHORTCUT_HOOKS.lock() {
                            if let Some(hook) = hooks.get(&id_for_check) {
                                if hook(&app_handle, &ctx) {
                                    return;
                                }
                            }
                        }

                        // ── 默认行为：显示主窗口 ──
                        if window_hidden {
                            crate::macos::webkit_tuning::show_main(&app_handle);
                        } else {
                            crate::macos::mac_utils::activate_app();
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
