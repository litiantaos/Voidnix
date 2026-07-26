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

/// 取 screenshot 窗口的 NSWindow 裸指针（None = 窗口不存在或 ns_window 失败）。
/// 调用方按需转 `*mut c_void` / `&NSWindow`（unsafe as_ref）/ `usize`。
/// 封装 get_webview_window + ns_window + cast 三步样板（session/pin/setup/scroll_capture 共用）。
#[cfg(target_os = "macos")]
pub(crate) fn screenshot_ns_window(app: &AppHandle) -> Option<*mut objc2_app_kit::NSWindow> {
    use tauri::Manager;
    let window = app.get_webview_window("screenshot")?;
    let raw = window.ns_window().ok()?;
    Some(raw.cast::<objc2_app_kit::NSWindow>())
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

/// 按需创建截图窗口（首次触发截图时调用）。已存在则跳过。
/// 创建后立即配置 CALayer / NSWindow level / collection behavior 等（原 setup::configure_overlay_window）。
#[cfg(target_os = "macos")]
pub(crate) fn ensure_screenshot_window(app: &AppHandle) -> bool {
    use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
    if app.get_webview_window("screenshot").is_some() {
        return true;
    }
    let url = WebviewUrl::App("index.html".into());
    let builder = WebviewWindowBuilder::new(app, "screenshot", url)
        .title("")
        .inner_size(800.0, 600.0)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .visible(false);
    if builder.build().is_err() {
        return false;
    }
    // 新窗口创建后配置原生层（背景 CALayer + NSWindow 属性）
    setup::configure_overlay_window(app);
    true
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn ensure_screenshot_window(_app: &AppHandle) -> bool {
    false
}

#[tauri::command]
pub async fn open_extension_subview(
    app: tauri::AppHandle,
    ext_id: String,
    subview_id: String,
    payload: serde_json::Value,
) -> Result<(), String> {
    use tauri::Emitter;

    let event_payload = serde_json::json!({
        "extId": ext_id,
        "subviewId": subview_id,
        "payload": payload,
        "wasVisible": crate::runtime::shortcut::is_window_visible(),
    });

    // show 由前端 listener 控制（wasVisible=false 时 setActiveExtension → rAF → showWindow），
    // 避免 Rust 立即 show 时 webview 仍为旧视图的闪现
    let _ = app.emit("open-extension-subview", event_payload);

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
        #[cfg(target_os = "macos")]
        {
            // 窗口创建（WKWebView ~45ms）重量级，deferred 到 bootstrap 后执行不阻塞 join_all。
            // AppKit 调用必须在主线程：spawn_blocking 内用 run_on_main_thread 调度。
            let app = _app.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let (tx, rx) = std::sync::mpsc::channel();
                let app2 = app.clone();
                let _ = app.run_on_main_thread(move || {
                    setup::ensure_and_configure_screenshot_window(&app2);
                    setup::install_reactivate_observer(&app2);
                    setup::schedule_jpeg_prewarm(&app2);
                    setup::schedule_overlay_prewarm(&app2);
                    let _ = tx.send(());
                });
                let _ = rx.recv_timeout(std::time::Duration::from_secs(10));
            });
            setup::register_shortcut_hook();
        }
        Ok(())
    }
}
