use crate::runtime::registry::Tier1Extension;
use tauri::AppHandle;

#[cfg(target_os = "macos")]
mod ffi;
#[allow(unused_imports)]
#[cfg(target_os = "macos")]
pub use ffi::*;

mod crop;
pub mod ocr;
pub mod pin;
pub mod scroll_capture;
pub mod session;
#[cfg(target_os = "macos")]
mod setup;
pub use session::{capture_screen, reactivate_screenshot_window};

#[cfg(target_os = "macos")]
pub fn install_background_layer(window: &tauri::WebviewWindow) {
    use objc2_app_kit::NSWindow;
    let Ok(raw) = window.ns_window() else { return };
    let ptr = raw.cast::<NSWindow>() as *mut std::ffi::c_void;
    unsafe {
        ffi::voidnix_screenshot_install_background_layer(ptr);
    }
}

#[cfg(not(target_os = "macos"))]
pub fn install_background_layer(_window: &tauri::WebviewWindow) {}

#[tauri::command]
pub async fn open_module_subview(
    app: tauri::AppHandle,
    module_id: String,
    subview_id: String,
    payload: serde_json::Value,
) -> Result<(), String> {
    use tauri::Emitter;

    let event_payload = serde_json::json!({
        "moduleId": module_id,
        "subviewId": subview_id,
        "payload": payload,
    });

    let app_handle = app.clone();
    app.run_on_main_thread(move || {
        crate::runtime::window::show_main(&app_handle);
        let _ = app_handle.emit("open-module-subview", event_payload);
    })
    .map_err(|e| e.to_string())?;

    Ok(())
}

/// Screenshot 扩展。
///
/// 拥有覆盖屏幕的全屏 `screenshot` 窗口、ScreenCaptureKit 截图会话、
/// OCR/截长图/钉图等子能力，以及全局快捷键钩子。
pub struct Plugin;

impl Tier1Extension for Plugin {
    fn id(&self) -> &'static str {
        "screenshot"
    }

    fn on_setup(&self, _app: &AppHandle) -> tauri::Result<()> {
        #[cfg(target_os = "macos")]
        {
            setup::configure_overlay_window(_app);
            setup::install_reactivate_observer(_app);
            setup::schedule_jpeg_prewarm(_app);
            setup::register_shortcut_hook();
        }
        Ok(())
    }
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
