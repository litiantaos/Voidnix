use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::{LazyLock, Mutex, mpsc};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tauri::Emitter;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

static WINDOW_VISIBLE: AtomicBool = AtomicBool::new(false);
static LAST_SHOW_MS: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── 快捷键录制模式 ───────────────────────────────────────────────────────────
//
// 录制是「全局模态」：只要任何 ShortcutInput 正在录制，所有已注册的全局
// 快捷键回调都应抑制默认行为，把按下的键位转发回前端由当前录制中的输入框
// 吸收。RECORDING_IDS 仅作前端调试/将来扩展用途，判断是否处于录制态以
// IS_RECORDING_ANY 为准（原子操作，回调热路径无需加锁）。

static RECORDING_IDS: LazyLock<Mutex<HashSet<String>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));
static IS_RECORDING_ANY: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub fn start_shortcut_recording(id: String) {
    if let Ok(mut set) = RECORDING_IDS.lock() {
        set.insert(id);
        IS_RECORDING_ANY.store(!set.is_empty(), Ordering::SeqCst);
    }
}

#[tauri::command]
pub fn stop_shortcut_recording(id: String) {
    if let Ok(mut set) = RECORDING_IDS.lock() {
        set.remove(&id);
        IS_RECORDING_ANY.store(!set.is_empty(), Ordering::SeqCst);
    }
}

// ── 扩展钩子注册 ─────────────────────────────────────────────────────────────

pub struct ShortcutContext {
    pub window_hidden: bool,
    #[cfg(target_os = "macos")]
    pub front_pid: Option<i32>,
}

type ShortcutHook = Box<dyn Fn(&tauri::AppHandle, &ShortcutContext) -> bool + Send + Sync>;

static SHORTCUT_HOOKS: LazyLock<Mutex<HashMap<String, ShortcutHook>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn register_shortcut_hook(id: &str, hook: ShortcutHook) {
    SHORTCUT_HOOKS.lock().unwrap().insert(id.to_string(), hook);
}

#[tauri::command]
pub fn is_app_active() -> bool {
    #[cfg(target_os = "macos")]
    return crate::platform::focus::is_app_active();
    #[cfg(not(target_os = "macos"))]
    true
}

/// 供窗口模块读写窗口可见状态。
pub(crate) fn set_window_visible(v: bool) {
    WINDOW_VISIBLE.store(v, Ordering::SeqCst);
    if v {
        LAST_SHOW_MS.store(now_ms(), Ordering::SeqCst);
    }
}

#[tauri::command]
pub fn hide_window(app: tauri::AppHandle, auto: Option<bool>) {
    if auto.unwrap_or(false) {
        let elapsed = now_ms().saturating_sub(LAST_SHOW_MS.load(Ordering::SeqCst));
        if elapsed < 500 {
            return;
        }
    }
    let _ = app.clone().run_on_main_thread(move || {
        crate::runtime::window::hide_main(&app);
    });
}

// ============================================================================
// Shortcut registration
// ============================================================================
//
// Rust 端是注册表的单一真相源：REGISTERED_BY_ID 按业务 id 维护「当前已注册
// 的 Shortcut」。前端只传 (id, new_shortcut)，旧值由 Rust 自查并 unregister，
// 避免前端通过 watch oldVal 推断旧值带来的撕裂。

static REGISTERED_BY_ID: LazyLock<Mutex<HashMap<String, Shortcut>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[tauri::command]
pub async fn register_global_shortcut(
    app: tauri::AppHandle,
    id: String,
    shortcut: String,
) -> Result<(), String> {
    let (tx, rx) = mpsc::sync_channel::<Result<(), String>>(1);
    let app_clone = app.clone();

    app.run_on_main_thread(move || {
        // 先 unregister 该 id 上一次注册的快捷键（如果有）
        if let Some(old_sc) = REGISTERED_BY_ID.lock().unwrap().remove(&id) {
            let _ = app_clone.global_shortcut().unregister(old_sc);
        }

        // 空字符串视为「仅注销」
        if shortcut.is_empty() {
            let _ = tx.send(Ok(()));
            return;
        }

        let new_sc = match Shortcut::from_str(&shortcut) {
            Ok(sc) => sc,
            Err(e) => {
                let _ = tx.send(Err(format!("parse '{}' failed: {:?}", shortcut, e)));
                return;
            }
        };

        let shortcut_id = id.clone();
        let shortcut_str = shortcut.clone();

        let result = app_clone.global_shortcut().on_shortcut(
            new_sc,
            move |app, _shortcut, event| {
                if event.state() != ShortcutState::Pressed {
                    return;
                }

                // 录制模态：任何 ShortcutInput 处于录制态，统一抑制默认
                // 行为并把按键字符串转发给前端，由当前录制中的输入框吸收。
                if IS_RECORDING_ANY.load(Ordering::SeqCst) {
                    let _ = app.emit(
                        "shortcut-recording-captured",
                        serde_json::json!({
                            "shortcut": shortcut_str.clone(),
                        }),
                    );
                    return;
                }

                let was_visible = WINDOW_VISIBLE.load(Ordering::SeqCst);
                let _ = app.emit(
                    "shortcut-pressed",
                    serde_json::json!({
                        "id": shortcut_id,
                        "wasVisible": was_visible,
                    }),
                );

                let app_handle = app.clone();
                let id_for_check = shortcut_id.clone();

                let _ = app.run_on_main_thread(move || {
                    let window_hidden = !was_visible;

                    #[cfg(target_os = "macos")]
                    let front_pid: Option<i32> = {
                        let ws = objc2_app_kit::NSWorkspace::sharedWorkspace();
                        ws.frontmostApplication().map(|a| a.processIdentifier())
                    };
                    #[cfg(not(target_os = "macos"))]
                    let front_pid: Option<i32> = None;

                    let ctx = ShortcutContext { window_hidden, front_pid };

                    if window_hidden {
                        #[cfg(target_os = "macos")]
                        crate::platform::focus::capture_frontmost();
                    }

                    if let Ok(hooks) = SHORTCUT_HOOKS.lock() {
                        if let Some(hook) = hooks.get(&id_for_check) {
                            if hook(&app_handle, &ctx) {
                                return;
                            }
                        }
                    }

                    if window_hidden {
                        crate::runtime::window::show_main(&app_handle);
                    }
                });
            },
        );

        match result {
            Ok(_) => {
                REGISTERED_BY_ID.lock().unwrap().insert(id, new_sc);
                let _ = tx.send(Ok(()));
            }
            Err(e) => {
                let _ = tx.send(Err(e.to_string()));
            }
        }
    })
    .map_err(|e| e.to_string())?;

    rx.recv().map_err(|e| e.to_string())?
}


pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new("shortcut")
        .build()
}
