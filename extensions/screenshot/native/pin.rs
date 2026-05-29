use super::crop::crop_with_annotation;
use super::ffi::{
    decode_image_data, png_bytes_to_cgimage,
    voidnix_screenshot_install_background_layer, voidnix_screenshot_set_background,
    CGImageRelease,
};

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

        let cg_addr = png_bytes_to_cgimage(&png) as usize;

        let path_str = path.to_string_lossy().to_string();
        let win_w = sel_w;
        let win_h = sel_h;
        let win_x = sel_x;
        let win_y = sel_y;
        let label = format!("pin-{}", ts);

        let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let app_clone = app.clone();
        app.run_on_main_thread(move || {
            let cg_ptr = cg_addr as *mut std::ffi::c_void;
            let r = create_pin_webview(
                &app_clone,
                &label,
                &path_str,
                win_w,
                win_h,
                win_x,
                win_y,
                cg_ptr,
            );
            let _ = tx.send(r);
        })
        .map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())??;
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, ann);
        return Err("仅支持 macOS".to_string());
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn create_pin_webview(
    app: &tauri::AppHandle,
    label: &str,
    image_path: &str,
    width: f64,
    height: f64,
    pos_x: f64,
    pos_y: f64,
    cg_image: *mut std::ffi::c_void,
) -> Result<(), String> {
    use objc2_app_kit::{NSWindow, NSWindowCollectionBehavior};
    use tauri::{WebviewUrl, WebviewWindowBuilder};

    let url_path = format!("/?img={}&pin=1", urlencoding::encode(image_path));
    let url = WebviewUrl::App(url_path.into());

    let builder = WebviewWindowBuilder::new(app, label, url)
        .title("")
        .inner_size(width, height)
        .position(pos_x, pos_y)
        .min_inner_size(80.0, 80.0)
        .resizable(true)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(true)
        .visible(true)
        .accept_first_mouse(true);

    let window = builder.build().map_err(|e| e.to_string())?;

    if let Ok(raw) = window.ns_window().map(|p| p.cast::<NSWindow>()) {
        unsafe {
            if let Some(ns) = raw.as_ref() {
                let _: () = objc2::msg_send![ns, setHidesOnDeactivate: false];
                ns.setLevel(objc2_app_kit::NSStatusWindowLevel);
                let behavior = NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::FullScreenAuxiliary
                    | NSWindowCollectionBehavior::Stationary;
                ns.setCollectionBehavior(behavior);
                ns.setContentAspectRatio(objc2_foundation::NSSize::new(width, height));
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
                if !cg_image.is_null() {
                    voidnix_screenshot_install_background_layer(ns_window_void);
                    voidnix_screenshot_set_background(ns_window_void, cg_image);
                    CGImageRelease(cg_image);
                }

                let _: () = objc2::msg_send![
                    ns,
                    makeKeyAndOrderFront: std::ptr::null::<objc2::runtime::AnyObject>()
                ];
            }
        }
    }
    crate::macos::mac_utils::activate_app();

    Ok(())
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
