#[cfg(target_os = "macos")]
mod ffi;
#[allow(unused_imports)]
pub use ffi::*;

mod crop;
pub mod ocr;
pub mod pin;
pub mod scroll_capture;
pub mod session;
pub use session::{capture_screen, reactivate_screenshot_window};

#[cfg(target_os = "macos")]
pub fn install_background_layer(window: &tauri::WebviewWindow) {
    use objc2_app_kit::NSWindow;
    let Ok(raw) = window.ns_window() else { return };
    let ptr = raw.cast::<NSWindow>() as *mut std::ffi::c_void;
    unsafe { ffi::voidnix_screenshot_install_background_layer(ptr); }
}

#[cfg(not(target_os = "macos"))]
pub fn install_background_layer(_window: &tauri::WebviewWindow) {}

#[tauri::command]
pub async fn open_module_panel(
    app: tauri::AppHandle,
    module_id: String,
    panel_id: String,
    payload: serde_json::Value,
) -> Result<(), String> {
    use tauri::Emitter;

    let event_payload = serde_json::json!({
        "moduleId": module_id,
        "panelId": panel_id,
        "payload": payload,
    });

    let app_handle = app.clone();
    app.run_on_main_thread(move || {
        crate::macos::webkit_tuning::show_main(&app_handle);
        let _ = app_handle.emit("open-module-panel", event_payload);
    })
    .map_err(|e| e.to_string())?;

    Ok(())
}

pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new("screenshot")
        .setup(|_app, _api| {
            #[cfg(target_os = "macos")]
            {
                use tauri::Emitter;
                crate::core::shortcut::register_shortcut_hook("screenshot", Box::new(|app, _ctx| {
                    if session::IS_IN_SCREENSHOT_SESSION.swap(true, std::sync::atomic::Ordering::SeqCst) {
                        return true;
                    }
                    let app_clone = app.clone();
                    std::thread::spawn(move || {
                        let result = capture_screen();
                        match result {
                            Ok(data) => {
                                let app_for_enter = app_clone.clone();
                                let _ = app_clone.run_on_main_thread(move || {
                                    session::enter_screenshot_mode_sync(&app_for_enter, data);
                                });
                            }
                            Err(e) => {
                                session::IS_IN_SCREENSHOT_SESSION.store(false, std::sync::atomic::Ordering::SeqCst);
                                let _ = app_clone.emit("screenshot-ready-error", e);
                            }
                        }
                    });
                    true
                }));
            }
            Ok(())
        })
        .build()
}

pub(crate) fn cleanup_temp_files() {
    let temp_dir = std::env::temp_dir();
    if let Ok(entries) = std::fs::read_dir(&temp_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("voidnix_") && (name.ends_with(".png") || name.ends_with(".jpg")) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    let awake_dir = temp_dir.join("com.litiantao.voidnix");
    let awake_bin = awake_dir.join("Display Wakelock");
    let _ = std::fs::remove_file(&awake_bin);
    let _ = std::fs::remove_dir(&awake_dir);
}
