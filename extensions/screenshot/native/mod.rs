use crate::runtime::registry::Extension;
use tauri::AppHandle;

#[cfg(target_os = "macos")]
mod ffi;

mod crop;
pub mod ocr;
pub mod pin;
pub mod scroll_capture;
pub mod session;
#[cfg(target_os = "macos")]
mod setup;
pub use session::{capture_screen, reactivate_screenshot_window};

/// 命令注册（局部 invoke_handler，§2.8）。
pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new("screenshot").build()
}

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
pub struct ScreenshotExtension;

#[async_trait::async_trait]
impl Extension for ScreenshotExtension {
    fn id(&self) -> &'static str {
        "screenshot"
    }

    async fn setup(&self, _app: &AppHandle) -> tauri::Result<()> {
        // /tmp 残留清理由 lib.rs setup 统一调 runtime::storage::cleanup_all_voidnix_temps()
        // （覆盖 screenshot + search 等所有扩展的 voidnix_* / voidnix-icon-* 残留）

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
