use super::crop::crop_with_annotation;
use super::ffi::{
    decode_image_data, png_bytes_to_cgimage, voidnix_screenshot_install_background_layer,
    voidnix_screenshot_set_background, voidnix_screenshot_set_background_centered, CGImageRelease,
};

use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Mutex;

// 关闭按钮 28 + 上下/左右各 8 边距 = 44
const PIN_MIN_SIZE: f64 = 44.0;

// pin 窗口关闭时需要恢复焦点的目标 PID
// 由 screenshot exit_impl 写入，pin 创建时读取
pub(super) static PIN_PREV_PID: AtomicI32 = AtomicI32::new(0);

/// pin 窗口临时文件 guard 注册表：label → TempHandle。
/// 窗口创建成功后插入，WindowEvent::Destroyed 时移除（Drop 自动删文件，§2.7）。
static PIN_TEMPS: std::sync::LazyLock<Mutex<HashMap<String, crate::runtime::storage::TempHandle>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

#[tauri::command]
pub async fn pin_image(
    app: tauri::AppHandle,
    sel_x: f64,
    sel_y: f64,
    sel_w: f64,
    sel_h: f64,
    scale: f64,
    annotation_png: String,
) -> Result<(), String> {
    let ann = if annotation_png.is_empty() {
        None
    } else {
        Some(decode_image_data(&annotation_png)?)
    };

    #[cfg(target_os = "macos")]
    {
        let png = crop_with_annotation(sel_x, sel_y, sel_w, sel_h, scale, ann.as_deref())?;
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let path = std::env::temp_dir().join(format!("voidnix_pin_{}.png", ts));
        std::fs::write(&path, &png).map_err(|e| e.to_string())?;
        // TempHandle：窗口创建失败时 Drop 清理；成功后转入 PIN_TEMPS，窗口 destroy 时清理（§2.7）
        let pin_handle = crate::runtime::storage::TempHandle::new(path.clone());

        let cg_addr = png_bytes_to_cgimage(&png) as usize;

        let path_str = path.to_string_lossy().to_string();
        // 截图小于窗口最小尺寸时，窗口保持最小尺寸，原图居中显示；否则窗口贴合截图尺寸。
        let centered = sel_w < PIN_MIN_SIZE || sel_h < PIN_MIN_SIZE;
        let win_w = sel_w.max(PIN_MIN_SIZE);
        let win_h = sel_h.max(PIN_MIN_SIZE);
        let win_x = sel_x - (win_w - sel_w) / 2.0;
        let win_y = sel_y - (win_h - sel_h) / 2.0;
        let label = format!("pin-{}", ts);
        let label_key = label.clone();

        let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let app_clone = app.clone();
        app.run_on_main_thread(move || {
            let cg_ptr = cg_addr as *mut std::ffi::c_void;
            let spec = PinWebviewSpec {
                label: &label,
                image_path: &path_str,
                width: win_w,
                height: win_h,
                pos_x: win_x,
                pos_y: win_y,
                cg_image: cg_ptr,
                centered,
            };
            let r = create_pin_webview(&app_clone, &spec);
            let _ = tx.send(r);
        })
        .map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())??;

        // 窗口创建成功，转移 handle 到注册表（WindowEvent::Destroyed 时移除 → Drop 删文件）
        if let Ok(mut map) = PIN_TEMPS.lock() {
            map.insert(label_key, pin_handle);
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, ann);
        return Err("仅支持 macOS".to_string());
    }

    Ok(())
}

#[cfg(target_os = "macos")]
struct PinWebviewSpec<'a> {
    label: &'a str,
    image_path: &'a str,
    width: f64,
    height: f64,
    pos_x: f64,
    pos_y: f64,
    cg_image: *mut std::ffi::c_void,
    centered: bool,
}

#[cfg(target_os = "macos")]
fn create_pin_webview(app: &tauri::AppHandle, spec: &PinWebviewSpec) -> Result<(), String> {
    use objc2_app_kit::{NSWindow, NSWindowCollectionBehavior};
    use tauri::{WebviewUrl, WebviewWindowBuilder};

    let url_path = format!("/?img={}&pin=1", urlencoding::encode(spec.image_path));
    let url = WebviewUrl::App(url_path.into());

    let builder = WebviewWindowBuilder::new(app, spec.label, url)
        .title("")
        .inner_size(spec.width, spec.height)
        .position(spec.pos_x, spec.pos_y)
        .min_inner_size(PIN_MIN_SIZE, PIN_MIN_SIZE)
        .resizable(true)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(true)
        .visible(true)
        .accept_first_mouse(true);

    let window = builder.build().map_err(|e| e.to_string())?;

    // 窗口销毁时移除 PIN_TEMPS 条目 → Drop TempHandle → 删除临时 PNG（§2.7）
    let label_for_destroy = spec.label.to_string();
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Destroyed) {
            if let Ok(mut map) = PIN_TEMPS.lock() {
                map.remove(&label_for_destroy);
            }
        }
    });

    if let Ok(raw) = window.ns_window().map(|p| p.cast::<NSWindow>()) {
        unsafe {
            if let Some(ns) = raw.as_ref() {
                let _: () = objc2::msg_send![ns, setHidesOnDeactivate: false];
                ns.setLevel(objc2_app_kit::NSStatusWindowLevel);
                let behavior = NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::FullScreenAuxiliary
                    | NSWindowCollectionBehavior::Stationary;
                ns.setCollectionBehavior(behavior);
                // 居中模式下窗口尺寸 ≠ 图片尺寸，不锁宽高比，避免后续 resize 还原成图片比例
                if !spec.centered {
                    ns.setContentAspectRatio(objc2_foundation::NSSize::new(
                        spec.width,
                        spec.height,
                    ));
                }
                if let Some(content_view) = ns.contentView() {
                    let _: () = objc2::msg_send![&content_view, setWantsLayer: true];
                    let layer: *mut objc2::runtime::AnyObject =
                        objc2::msg_send![&content_view, layer];
                    if !layer.is_null() {
                        let _: () = objc2::msg_send![layer, setCornerRadius: 10.0_f64];
                        let _: () = objc2::msg_send![layer, setMasksToBounds: true];
                    }
                }

                let ns_window_void = raw.cast::<NSWindow>() as *mut std::ffi::c_void;
                if !spec.cg_image.is_null() {
                    voidnix_screenshot_install_background_layer(ns_window_void);
                    if spec.centered {
                        voidnix_screenshot_set_background_centered(ns_window_void, spec.cg_image);
                    } else {
                        voidnix_screenshot_set_background(ns_window_void, spec.cg_image);
                    }
                    CGImageRelease(spec.cg_image);
                }

                let _: () = objc2::msg_send![
                    ns,
                    makeKeyAndOrderFront: std::ptr::null::<objc2::runtime::AnyObject>()
                ];
            }
        }
    }

    crate::platform::focus::activate_app();

    Ok(())
}

/// 恢复 pin 窗口关闭前的焦点应用。
#[tauri::command]
pub async fn restore_pin_focus(window: tauri::WebviewWindow) {
    let pid = PIN_PREV_PID.load(Ordering::SeqCst);
    if pid > 0 {
        // 先隐藏窗口，确保其不再是 key window
        let _ = window.hide();
        // 再 deactivate 触发系统重新评估 key window，恢复到原应用
        crate::platform::focus::deactivate_app();
        crate::platform::focus::activate_app_by_pid(pid);
        PIN_PREV_PID.store(0, Ordering::SeqCst);
    }
}

#[tauri::command]
pub async fn set_pin_window_opacity(
    window: tauri::WebviewWindow,
    opacity: f64,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::NSWindow;
        use tauri::Manager;
        let app_handle = window.app_handle().clone();
        let opacity_val = opacity.clamp(0.1, 1.0);
        app_handle
            .run_on_main_thread(move || {
                if let Ok(raw) = window.ns_window().map(|p| p.cast::<NSWindow>()) {
                    unsafe {
                        if let Some(ns) = raw.as_ref() {
                            ns.setAlphaValue(opacity_val);
                        }
                    }
                }
            })
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, opacity);
    }
    Ok(())
}

/// 查全局鼠标位置（屏幕坐标，左上原点，CSS 像素）。
/// pin 窗口失焦时 WKWebView 不派发 mouseenter/leave，前端 rAF 轮询此值
/// 自行计算 hover 状态。
#[tauri::command]
pub fn pin_global_mouse() -> (f64, f64) {
    #[cfg(target_os = "macos")]
    {
        let mut x = 0.0f64;
        let mut y = 0.0f64;
        unsafe {
            super::ffi::voidnix_screenshot_get_mouse_location(&mut x, &mut y, 0.0);
        }
        (x, y)
    }
    #[cfg(not(target_os = "macos"))]
    {
        (0.0, 0.0)
    }
}
