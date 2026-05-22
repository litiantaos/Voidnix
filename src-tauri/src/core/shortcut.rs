use std::collections::HashSet;
use std::str::FromStr;
use std::sync::{LazyLock, Mutex, mpsc};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Manager, Emitter};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

pub(crate) static SELECTED_TEXT: Mutex<String> = Mutex::new(String::new());
static WINDOW_VISIBLE: AtomicBool = AtomicBool::new(false);

// ── 快捷键录制模式 ───────────────────────────────────────────────────────────

static RECORDING_IDS: LazyLock<Mutex<HashSet<String>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

#[tauri::command]
pub fn start_shortcut_recording(id: String) {
    if let Ok(mut set) = RECORDING_IDS.lock() {
        set.insert(id);
    }
}

#[tauri::command]
pub fn stop_shortcut_recording(id: String) {
    if let Ok(mut set) = RECORDING_IDS.lock() {
        set.remove(&id);
    }
}

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
    let (tx, rx) = mpsc::sync_channel::<Option<String>>(1);
    let app_clone = app.clone();
    app.run_on_main_thread(move || {
        if let Some(old) = old_shortcut {
            if let Ok(old_sc) = Shortcut::from_str(&old) {
                let _ = app_clone.global_shortcut().unregister(old_sc);
            }
        }

        if new_shortcut.is_empty() {
            let _ = tx.send(None);
            return;
        }

        let registration_error = match Shortcut::from_str(&new_shortcut) {
            Ok(new_sc) => {
                let shortcut_id = id.clone();
                let shortcut_str = new_shortcut.clone();
                match app_clone.global_shortcut().on_shortcut(new_sc, move |app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        // 录制模式：已注册快捷键在录制期间触发 → 回传前端，跳过默认行为
                        if let Ok(recording) = RECORDING_IDS.lock() {
                            if recording.contains(&shortcut_id) {
                                let _ = app.emit(
                                    "shortcut-recording-captured",
                                    serde_json::json!({
                                        "id": shortcut_id.clone(),
                                        "shortcut": shortcut_str.clone(),
                                    }),
                                );
                                return;
                            }
                        }

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
                                ws.frontmostApplication().map(|a| a.processIdentifier())
                            };
                            #[cfg(not(target_os = "macos"))]
                            let front_pid: Option<i32> = None;

                            let ctx = ShortcutContext { window_hidden, front_pid };

                            if let Ok(hooks) = SHORTCUT_HOOKS.lock() {
                                if let Some(hook) = hooks.get(&id_for_check) {
                                    if hook(&app_handle, &ctx) {
                                        return;
                                    }
                                }
                            }

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
                }) {
                    Ok(_) => None,
                    Err(e) => Some(e.to_string()),
                }
            }
            Err(e) => Some(format!("parse '{}' failed: {:?}", new_shortcut, e)),
        };

        let _ = tx.send(registration_error);
    }).map_err(|e| e.to_string())?;

    match rx.recv() {
        Ok(Some(err)) => Err(err),
        Ok(None) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}


pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new("shortcut")
        .build()
}
