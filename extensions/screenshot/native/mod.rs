use crate::runtime::registry::Extension;
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

/// 命令注册（局部 invoke_handler，§2.8）。
pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new("screenshot")
        .invoke_handler(tauri::generate_handler![
            open_module_subview,
            session::capture_screen,
            session::enter_screenshot_mode,
            session::screenshot_overlay_ready,
            session::exit_screenshot_mode,
            pin::pin_image,
            pin::restore_pin_focus,
            pin::set_pin_window_opacity,
            pin::pin_global_mouse,
            scroll_capture::enter_scroll_capture,
            scroll_capture::exit_scroll_capture,
            scroll_capture::set_scroll_toolbar_rect,
            scroll_capture::finish_scroll_capture,
            scroll_capture::save_scroll_result,
            scroll_capture::copy_scroll_result_to_clipboard,
            ocr::ocr_image,
            ocr::detect_text_regions,
            ocr::save_screenshot,
            ocr::copy_screenshot_to_clipboard,
        ])
        .build()
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
        // 清理上次会话遗留的临时文件
        cleanup_temp_files();

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

/// 清理上次会话遗留的临时文件（启动时调用，委托至 runtime::storage）。
fn cleanup_temp_files() {
    let temp_dir = std::env::temp_dir();
    crate::runtime::storage::cleanup_temps_by_prefix(&temp_dir, "voidnix_", &[".png", ".jpg"]);
    // 兼容：清理旧版 awake binary（已迁移至 app_data_dir）
    let awake_dir = temp_dir.join("com.litiantao.voidnix");
    let _ = std::fs::remove_file(awake_dir.join("Display Wakelock"));
    let _ = std::fs::remove_dir(&awake_dir);
}
