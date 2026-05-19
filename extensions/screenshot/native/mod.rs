use base64::Engine;
use serde::{Deserialize, Serialize};
use std::process::Command;
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicI32, Ordering};

/// 截屏前的前台应用 PID，退出截屏时用于恢复焦点。
#[cfg(target_os = "macos")]
static PREV_FRONT_PID: AtomicI32 = AtomicI32::new(0);

/// 全局持有最近一次截屏的 CGImageRef（裸指针，CFRetain 管理生命周期）。
#[cfg(target_os = "macos")]
struct SendCgImage(*mut std::ffi::c_void);
#[cfg(target_os = "macos")]
unsafe impl Send for SendCgImage {}
#[cfg(target_os = "macos")]
unsafe impl Sync for SendCgImage {}

#[cfg(target_os = "macos")]
static CURRENT_CG_IMAGE: std::sync::Mutex<SendCgImage> =
    std::sync::Mutex::new(SendCgImage(std::ptr::null_mut()));
/// 屏幕上某个窗口的可见区域（CSS 像素，左上原点），用于截屏时智能识别。
#[derive(Serialize, Deserialize, Clone, Debug)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct WindowRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub owner: String,
}

/// 截屏元数据（不再含 data_url，背景图由 CALayer 直贴）。
/// data_url 字段保留但置空，前端不再用它加载背景图。
#[derive(Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ScreenshotData {
    /// 已废弃：CALayer 直贴方案下置空。保留字段避免前端类型报错。
    pub data_url: String,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
    pub mouse_x: f64,
    pub mouse_y: f64,
    pub windows: Vec<WindowRect>,
}

// ── CALayer 桥接（来自 native/screenshot_overlay.mm）────────────────────────
#[cfg(target_os = "macos")]
extern "C" {
    fn voidnix_screenshot_install_background_layer(ns_window_ptr: *mut std::ffi::c_void) -> bool;
    fn voidnix_screenshot_set_background(
        ns_window_ptr: *mut std::ffi::c_void,
        cg_image_ptr: *mut std::ffi::c_void,
    ) -> bool;
    fn voidnix_screenshot_clear_background(ns_window_ptr: *mut std::ffi::c_void);
}

// ── CGImage 生命周期管理 ─────────────────────────────────────────────────────
#[cfg(target_os = "macos")]
extern "C" {
    fn CGImageRetain(image: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    fn CGImageRelease(image: *mut std::ffi::c_void);
}

/// 把 CGImage 存入全局，CFRetain 新值，CFRelease 旧值。
#[cfg(target_os = "macos")]
fn store_cg_image(raw: *mut std::ffi::c_void) {
    let mut guard = CURRENT_CG_IMAGE.lock().unwrap();
    let old = guard.0;
    if !old.is_null() { unsafe { CGImageRelease(old) }; }
    if !raw.is_null() { unsafe { CGImageRetain(raw) }; }
    guard.0 = raw;
}

/// 取出当前 CGImage 裸指针（不改变引用计数，调用方不得 release）。
#[cfg(target_os = "macos")]
fn get_cg_image() -> *mut std::ffi::c_void {
    CURRENT_CG_IMAGE.lock().unwrap().0
}

// ── 截屏 ─────────────────────────────────────────────────────────────────────

/// 截取主屏幕，持有 CGImageRef（不编码、不写文件），返回元数据。
/// 背景图由 enter_impl 通过 CALayer 直贴，零编码、零拷贝、零 IPC。
#[tauri::command]
pub fn capture_screen() -> Result<ScreenshotData, String> {
    use core_graphics::display::CGDisplay;

    let display = CGDisplay::main();
    let bounds = display.bounds();
    let lw = bounds.size.width as u32;
    let lh = bounds.size.height as u32;

    let cg_image = display.image().ok_or(
        "CGDisplayCreateImage 失败：请在「系统设置 → 隐私与安全性 → 屏幕录制」中授权",
    )?;

    let pw = cg_image.width() as u32;
    let scale = if lw > 0 { pw as f64 / lw as f64 } else { 1.0 };

    // 取出 CGImageRef 裸指针并全局持有（CFRetain）
    // picker JPEG 在 enter_impl 主线程生成（NSBitmapImageRep initWithCGImage: 需要主线程）
    #[cfg(target_os = "macos")]
    {
        let raw: *mut std::ffi::c_void = unsafe { std::mem::transmute_copy(&cg_image) };
        store_cg_image(raw);
    }

    let windows = enumerate_visible_windows();

    Ok(ScreenshotData {
        data_url: String::new(), // CALayer 直贴，不需要文件路径
        width: lw,
        height: lh,
        scale,
        mouse_x: 0.0,
        mouse_y: 0.0,
        windows,
    })
}

// ── 放大镜 JPEG（异步，不阻塞按键到显示路径）────────────────────────────────

/// 放大镜用的 JPEG 文件路径，前端异步 fetch 加载。
pub fn picker_jpeg_path() -> std::path::PathBuf {
    std::env::temp_dir().join("voidnix_picker.jpg")
}

/// 把当前 CGImage 编码为 JPEG 写入临时文件，供前端放大镜取色用。
/// 在后台线程调用，不阻塞按键到显示路径。
#[cfg(target_os = "macos")]
fn prepare_picker_jpeg(raw: *mut std::ffi::c_void) {
    if raw.is_null() {
        eprintln!("[shot] prepare_picker_jpeg: raw is null");
        return;
    }
    match cg_image_ptr_to_jpeg(raw) {
        Ok(data) => {
            if let Err(e) = std::fs::write(picker_jpeg_path(), &data) {
                eprintln!("[shot] prepare_picker_jpeg write error: {}", e);
            }
        }
        Err(e) => eprintln!("[shot] prepare_picker_jpeg encode error: {}", e),
    }
}

// ── 枚举可见窗口 ─────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn enumerate_visible_windows() -> Vec<WindowRect> {
    use core_foundation::array::{CFArray, CFArrayRef};
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use std::ffi::c_void;

    type CGWindowListOption = u32;
    const KCG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY: CGWindowListOption = 1 << 0;
    const KCG_WINDOW_LIST_EXCLUDE_DESKTOP_ELEMENTS: CGWindowListOption = 1 << 4;
    type CGWindowID = u32;
    extern "C" {
        fn CGWindowListCopyWindowInfo(
            option: CGWindowListOption,
            relativeToWindow: CGWindowID,
        ) -> CFArrayRef;
    }

    let raw = unsafe {
        CGWindowListCopyWindowInfo(
            KCG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY | KCG_WINDOW_LIST_EXCLUDE_DESKTOP_ELEMENTS,
            0,
        )
    };
    if raw.is_null() { return Vec::new(); }
    let array: CFArray<CFDictionary<*const c_void, *const c_void>> =
        unsafe { CFArray::wrap_under_create_rule(raw) };

    let self_pid = std::process::id() as i64;
    let mut result = Vec::with_capacity(array.len() as usize);

    let key_layer  = CFString::from_static_string("kCGWindowLayer");
    let key_bounds = CFString::from_static_string("kCGWindowBounds");
    let key_pid    = CFString::from_static_string("kCGWindowOwnerPID");
    let key_name   = CFString::from_static_string("kCGWindowOwnerName");
    let key_alpha  = CFString::from_static_string("kCGWindowAlpha");

    let lookup = |dict: &CFDictionary<*const c_void, *const c_void>, key: &CFString| -> Option<CFType> {
        let ptr = key.as_concrete_TypeRef() as *const c_void;
        let v = dict.find(ptr)?;
        if v.is_null() { return None; }
        Some(unsafe { CFType::wrap_under_get_rule(*v as _) })
    };

    for i in 0..array.len() {
        let Some(dict) = array.get(i) else { continue };

        let layer = lookup(&dict, &key_layer)
            .and_then(|v| v.downcast::<CFNumber>()).and_then(|n| n.to_i64()).unwrap_or(-1);
        if layer != 0 { continue; }

        let pid = lookup(&dict, &key_pid)
            .and_then(|v| v.downcast::<CFNumber>()).and_then(|n| n.to_i64()).unwrap_or(0);
        if pid == self_pid { continue; }

        let alpha = lookup(&dict, &key_alpha)
            .and_then(|v| v.downcast::<CFNumber>()).and_then(|n| n.to_f64()).unwrap_or(1.0);
        if alpha < 0.05 { continue; }

        let owner = lookup(&dict, &key_name)
            .and_then(|v| v.downcast::<CFString>()).map(|s| s.to_string()).unwrap_or_default();

        let Some(bd) = lookup(&dict, &key_bounds)
            .and_then(|v| v.downcast::<CFDictionary<*const c_void, *const c_void>>())
        else { continue };

        let gn = |k: &'static str| -> f64 {
            let ks = CFString::from_static_string(k);
            lookup(&bd, &ks).and_then(|v| v.downcast::<CFNumber>()).and_then(|n| n.to_f64()).unwrap_or(0.0)
        };
        let (x, y, w, h) = (gn("X"), gn("Y"), gn("Width"), gn("Height"));
        if w < 40.0 || h < 40.0 { continue; }
        result.push(WindowRect { x, y, w, h, owner });
    }
    result
}

#[cfg(not(target_os = "macos"))]
fn enumerate_visible_windows() -> Vec<WindowRect> { Vec::new() }

// ── JPEG 编码（放大镜用）────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn cg_image_ptr_to_jpeg(raw: *mut std::ffi::c_void) -> Result<Vec<u8>, String> {
    use objc2::runtime::AnyObject;
    if raw.is_null() { return Err("CGImage 为空".to_string()); }
    unsafe {
        let cls = objc2::class!(NSBitmapImageRep);
        let rep: *mut AnyObject = objc2::msg_send![cls, alloc];
        let rep: *mut AnyObject = objc2::msg_send![rep, initWithCGImage: raw];
        if rep.is_null() { return Err("NSBitmapImageRep initWithCGImage 失败".to_string()); }
        let key = objc2_foundation::NSString::from_str("NSImageCompressionFactor");
        let val: *mut AnyObject = objc2::msg_send![objc2::class!(NSNumber), numberWithDouble: 0.95f64];
        let props: *mut AnyObject = objc2::msg_send![
            objc2::class!(NSDictionary), dictionaryWithObject: val, forKey: &*key
        ];
        let ns_data: *mut AnyObject = objc2::msg_send![rep, representationUsingType: 3usize, properties: props];
        let _: () = objc2::msg_send![rep, release];
        if ns_data.is_null() { return Err("JPEG 编码失败".to_string()); }
        let length: usize = objc2::msg_send![ns_data, length];
        let bytes: *const u8 = objc2::msg_send![ns_data, bytes];
        Ok(std::slice::from_raw_parts(bytes, length).to_vec())
    }
}

// ── 预热 ─────────────────────────────────────────────────────────────────────

/// 启动时预热 JPEG 编码器（放大镜路径），让首次编码不付出 lazy 加载代价。
#[cfg(target_os = "macos")]
pub fn prewarm_jpeg_encoder() {
    use objc2::runtime::AnyObject;
    unsafe {
        let cls = objc2::class!(NSBitmapImageRep);
        let rep: *mut AnyObject = objc2::msg_send![cls, alloc];
        let cs = objc2_foundation::NSString::from_str("NSDeviceRGBColorSpace");
        let null_planes: *mut *mut u8 = std::ptr::null_mut();
        let rep: *mut AnyObject = objc2::msg_send![
            rep,
            initWithBitmapDataPlanes: null_planes,
            pixelsWide: 1isize, pixelsHigh: 1isize,
            bitsPerSample: 8isize, samplesPerPixel: 4isize,
            hasAlpha: true, isPlanar: false,
            colorSpaceName: &*cs,
            bytesPerRow: 0isize, bitsPerPixel: 0isize
        ];
        if rep.is_null() { return; }
        let key = objc2_foundation::NSString::from_str("NSImageCompressionFactor");
        let val: *mut AnyObject = objc2::msg_send![objc2::class!(NSNumber), numberWithDouble: 0.95f64];
        let props: *mut AnyObject = objc2::msg_send![
            objc2::class!(NSDictionary), dictionaryWithObject: val, forKey: &*key
        ];
        let _: *mut AnyObject = objc2::msg_send![rep, representationUsingType: 3usize, properties: props];
        let _: () = objc2::msg_send![rep, release];
    }
}

#[cfg(not(target_os = "macos"))]
pub fn prewarm_jpeg_encoder() {}

// ── CALayer 安装（lib.rs setup 阶段调用）────────────────────────────────────

/// 在 screenshot 窗口的 contentView 下方安装 backgroundLayer。
/// 幂等，可重复调用。
#[cfg(target_os = "macos")]
pub fn install_background_layer(window: &tauri::WebviewWindow) {
    use objc2_app_kit::NSWindow;
    let Ok(raw) = window.ns_window() else { return };
    let ptr = raw.cast::<NSWindow>() as *mut std::ffi::c_void;
    unsafe { voidnix_screenshot_install_background_layer(ptr); }
}

#[cfg(not(target_os = "macos"))]
pub fn install_background_layer(_window: &tauri::WebviewWindow) {}

// ── OCR / 复制 / 保存（从持有的 CGImage 裁剪，不依赖前端 dataURL）──────────

/// 从当前持有的 CGImage 裁剪选区，合并标注 PNG，返回 PNG 字节。
/// 供 OCR / 复制 / 保存共用。
#[cfg(target_os = "macos")]
fn crop_with_annotation(
    sel_x: f64, sel_y: f64, sel_w: f64, sel_h: f64,
    scale: f64,
    annotation_png: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    use core_graphics::geometry::{CGPoint, CGRect, CGSize};
    use objc2::runtime::AnyObject;

    let raw = get_cg_image();
    if raw.is_null() { return Err("无截屏数据".to_string()); }

    // CGImageCreateWithImageInRect 裁剪（物理像素坐标）
    // CGDisplayCreateImage 返回的图像与屏幕像素顺序一致，左上原点，无需翻转。
    extern "C" {
        fn CGImageCreateWithImageInRect(
            image: *mut std::ffi::c_void,
            rect: CGRect,
        ) -> *mut std::ffi::c_void;
    }
    let rect = CGRect {
        origin: CGPoint { x: sel_x * scale, y: sel_y * scale },
        size: CGSize { width: sel_w * scale, height: sel_h * scale },
    };
    let cropped = unsafe { CGImageCreateWithImageInRect(raw, rect) };
    if cropped.is_null() { return Err("CGImageCreateWithImageInRect 失败".to_string()); }

    // 合成：把裁剪图 + 标注 PNG 画到 NSBitmapImageRep，输出 PNG
    let result = unsafe {
        // 创建 NSBitmapImageRep（物理像素尺寸）
        let pw = (sel_w * scale) as isize;
        let ph = (sel_h * scale) as isize;
        let cls = objc2::class!(NSBitmapImageRep);
        let rep: *mut AnyObject = objc2::msg_send![cls, alloc];
        let cs = objc2_foundation::NSString::from_str("NSDeviceRGBColorSpace");
        let null_planes: *mut *mut u8 = std::ptr::null_mut();
        let rep: *mut AnyObject = objc2::msg_send![
            rep,
            initWithBitmapDataPlanes: null_planes,
            pixelsWide: pw, pixelsHigh: ph,
            bitsPerSample: 8isize, samplesPerPixel: 4isize,
            hasAlpha: true, isPlanar: false,
            colorSpaceName: &*cs,
            bytesPerRow: 0isize, bitsPerPixel: 0isize
        ];
        if rep.is_null() {
            CGImageRelease(cropped);
            return Err("NSBitmapImageRep alloc 失败".to_string());
        }

        // 关键：把 rep 的"点尺寸"声明为逻辑像素，绘图坐标系即可用逻辑像素，
        // 系统自动把绘制结果按 scale 渲染到物理像素画布。
        let _: () = objc2::msg_send![
            rep, setSize: objc2_foundation::NSSize::new(sel_w, sel_h)
        ];

        // NSGraphicsContext.currentContext = [NSGraphicsContext graphicsContextWithBitmapImageRep:rep]
        let gc_cls = objc2::class!(NSGraphicsContext);
        let gc: *mut AnyObject = objc2::msg_send![gc_cls, graphicsContextWithBitmapImageRep: rep];
        let _: () = objc2::msg_send![gc_cls, saveGraphicsState];
        let _: () = objc2::msg_send![gc_cls, setCurrentContext: gc];

        // 画背景（裁剪后的 CGImage）
        let ns_image_cls = objc2::class!(NSImage);
        let bg_img: *mut AnyObject = objc2::msg_send![ns_image_cls, alloc];
        let bg_img: *mut AnyObject = objc2::msg_send![bg_img, initWithCGImage: cropped, size: objc2_foundation::NSSize::new(sel_w, sel_h)];
        if !bg_img.is_null() {
            let dst = objc2_foundation::NSRect::new(
                objc2_foundation::NSPoint::new(0.0, 0.0),
                objc2_foundation::NSSize::new(sel_w, sel_h),
            );
            let _: () = objc2::msg_send![bg_img, drawInRect: dst];
            let _: () = objc2::msg_send![bg_img, release];
        }

        // 叠加标注 PNG（如果有）
        if let Some(ann_bytes) = annotation_png {
            if !ann_bytes.is_empty() {
                let ns_data_cls = objc2::class!(NSData);
                let ns_data: *mut AnyObject = objc2::msg_send![
                    ns_data_cls,
                    dataWithBytes: ann_bytes.as_ptr() as *const std::ffi::c_void,
                    length: ann_bytes.len()
                ];
                let ann_img: *mut AnyObject = objc2::msg_send![ns_image_cls, alloc];
                let ann_img: *mut AnyObject = objc2::msg_send![ann_img, initWithData: ns_data];
                if !ann_img.is_null() {
                    let dst = objc2_foundation::NSRect::new(
                        objc2_foundation::NSPoint::new(0.0, 0.0),
                        objc2_foundation::NSSize::new(sel_w, sel_h),
                    );
                    let _: () = objc2::msg_send![ann_img, drawInRect: dst];
                    let _: () = objc2::msg_send![ann_img, release];
                }
            }
        }

        let _: () = objc2::msg_send![gc_cls, restoreGraphicsState];

        // 输出 PNG
        let props_cls = objc2::class!(NSDictionary);
        let props: *mut AnyObject = objc2::msg_send![props_cls, dictionary];
        // NSBitmapImageFileTypePNG = 4
        let ns_data: *mut AnyObject = objc2::msg_send![rep, representationUsingType: 4usize, properties: props];
        let _: () = objc2::msg_send![rep, release];
        CGImageRelease(cropped);

        if ns_data.is_null() {
            return Err("PNG 输出失败".to_string());
        }
        let length: usize = objc2::msg_send![ns_data, length];
        let bytes: *const u8 = objc2::msg_send![ns_data, bytes];
        std::slice::from_raw_parts(bytes, length).to_vec()
    };
    Ok(result)
}

// ── Tauri commands ────────────────────────────────────────────────────────────

/// OCR 识别选区文字（Apple Vision）。
/// annotation_png: 前端 canvas 标注层的 PNG base64（可为空）。
#[tauri::command]
#[cfg_attr(feature = "specta", specta::specta)]
pub async fn ocr_image(
    sel_x: f64, sel_y: f64, sel_w: f64, sel_h: f64,
    scale: f64,
    annotation_png: String,
) -> Result<String, String> {
    let ann = if annotation_png.is_empty() {
        None
    } else {
        Some(decode_image_data(&annotation_png)?)
    };

    #[cfg(target_os = "macos")]
    {
        let png = crop_with_annotation(sel_x, sel_y, sel_w, sel_h, scale, ann.as_deref())?;
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis();
        let tmp = std::env::temp_dir().join(format!("voidnix_ocr_{}.png", ts));
        std::fs::write(&tmp, &png).map_err(|e| e.to_string())?;

        let script = format!(
            r#"import Vision; import AppKit
let url = URL(fileURLWithPath: "{path}")
guard let img = NSImage(contentsOf: url), let cg = img.cgImage(forProposedRect: nil, context: nil, hints: nil) else {{ print(""); exit(0) }}
let req = VNRecognizeTextRequest()
req.recognitionLevel = .accurate; req.usesLanguageCorrection = true
req.recognitionLanguages = ["zh-Hans","zh-Hant","en-US","ja"]
try? VNImageRequestHandler(cgImage: cg, options: [:]).perform([req])
print((req.results ?? []).compactMap {{ $0.topCandidates(1).first?.string }}.joined(separator: "\n"))"#,
            path = tmp.display()
        );
        let out = Command::new("swift").args(["-e", &script]).output()
            .map_err(|e| format!("swift 失败: {}", e))?;
        let _ = std::fs::remove_file(&tmp);
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }
    #[cfg(not(target_os = "macos"))]
    Err("仅支持 macOS".to_string())
}

/// 保存选区（含标注）到文件。
/// path 可以是完整文件路径，也可以是目录路径（此时自动生成文件名）。
#[tauri::command]
pub async fn save_screenshot(
    sel_x: f64, sel_y: f64, sel_w: f64, sel_h: f64,
    scale: f64,
    annotation_png: String,
    path: String,
) -> Result<String, String> {
    let ann = if annotation_png.is_empty() { None } else { Some(decode_image_data(&annotation_png)?) };
    #[cfg(target_os = "macos")]
    {
        let png = crop_with_annotation(sel_x, sel_y, sel_w, sel_h, scale, ann.as_deref())?;
        // 如果 path 是目录，自动生成文件名
        let file_path = {
            let p = std::path::Path::new(&path);
            if p.is_dir() {
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
                p.join(format!("screenshot_{}.png", ts))
            } else {
                p.to_path_buf()
            }
        };
        // 确保父目录存在
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&file_path, png).map_err(|e| e.to_string())?;
        Ok(file_path.to_string_lossy().to_string())
    }
    #[cfg(not(target_os = "macos"))]
    Err("仅支持 macOS".to_string())
}

/// 复制选区（含标注）到剪贴板。
#[tauri::command]
pub async fn copy_screenshot_to_clipboard(
    sel_x: f64, sel_y: f64, sel_w: f64, sel_h: f64,
    scale: f64,
    annotation_png: String,
) -> Result<(), String> {
    let ann = if annotation_png.is_empty() { None } else { Some(decode_image_data(&annotation_png)?) };
    #[cfg(target_os = "macos")]
    {
        let png = crop_with_annotation(sel_x, sel_y, sel_w, sel_h, scale, ann.as_deref())?;
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis();
        let tmp = std::env::temp_dir().join(format!("voidnix_clip_{}.png", ts));
        std::fs::write(&tmp, &png).map_err(|e| e.to_string())?;
        let script = format!(
            "set f to POSIX file \"{}\"\nset the clipboard to (read f as «class PNGf»)",
            tmp.display()
        );
        let out = Command::new("osascript").args(["-e", &script]).output()
            .map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&tmp);
        if out.status.success() { Ok(()) } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }
    #[cfg(not(target_os = "macos"))]
    Err("仅支持 macOS".to_string())
}

fn decode_image_data(s: &str) -> Result<Vec<u8>, String> {
    let b64 = if let Some(p) = s.find(',') { &s[p + 1..] } else { s };
    base64::engine::general_purpose::STANDARD.decode(b64).map_err(|e| e.to_string())
}

// ── enter / exit ──────────────────────────────────────────────────────────────

/// Tauri command 入口（保留兼容，实际路径走 enter_screenshot_mode_sync）
#[tauri::command]
pub async fn enter_screenshot_mode(app: tauri::AppHandle, data: ScreenshotData) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
    let app_c = app.clone();
    app.run_on_main_thread(move || { let _ = tx.send(enter_impl(&app_c, &data)); })
        .map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())?
}

/// 主线程同步入口：capture 完成后直接调，省去 IPC 中转。
#[cfg(target_os = "macos")]
pub fn enter_screenshot_mode_sync(app: &tauri::AppHandle, data: ScreenshotData) {
    let _ = enter_impl(app, &data);
}
#[cfg(not(target_os = "macos"))]
pub fn enter_screenshot_mode_sync(_app: &tauri::AppHandle, _data: ScreenshotData) {}

#[cfg(target_os = "macos")]
fn enter_impl(app: &tauri::AppHandle, data: &ScreenshotData) -> Result<(), String> {
    use tauri::Manager;
    use objc2_app_kit::{NSEvent, NSScreen, NSWindow, NSWindowAnimationBehavior, NSWorkspace};
    use objc2_foundation::MainThreadMarker;

    let self_pid = std::process::id() as i32;
    let prev_pid = NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .map(|a| a.processIdentifier() as i32)
        .filter(|&p| p != self_pid)
        .unwrap_or(0);
    PREV_FRONT_PID.store(prev_pid, Ordering::SeqCst);

    if let Some(main_window) = app.get_webview_window("main") {
        let _ = main_window.hide();
    }
    crate::macos::click_monitor::remove();

    let window = app.get_webview_window("screenshot").ok_or("找不到截图窗口")?;
    let raw = window.ns_window().map_err(|e| e.to_string())?.cast::<NSWindow>();
    unsafe {
        let ns_window: &NSWindow = raw.as_ref().ok_or("NSWindow 为空")?;
        let mtm = MainThreadMarker::new().ok_or("不在主线程")?;

        let screen_height = if let Some(screen) = NSScreen::mainScreen(mtm) {
            let frame = screen.frame();
            ns_window.setFrame_display(frame, true);
            frame.size.height
        } else {
            data.height as f64
        };

        ns_window.setAnimationBehavior(NSWindowAnimationBehavior::None);

        // ── 工业级核心：CALayer 直贴 CGImage，零编码零拷贝 ──────────────────
        let ns_window_ptr = raw.cast::<NSWindow>() as *mut std::ffi::c_void;
        let cg_image_ptr = get_cg_image();
        if !cg_image_ptr.is_null() {
            voidnix_screenshot_set_background(ns_window_ptr, cg_image_ptr);
        }

        // SkyLight 迁移到当前 active Space（用 SLSAddWindowsToSpaces，
        // 全屏 Space 和普通桌面都支持，跨 macOS 13/14/15 稳定）
        let window_number: objc2_foundation::NSInteger = objc2::msg_send![ns_window, windowNumber];
        let _ = crate::macos::skylight::move_window_to_active_space(window_number as i64, ns_window_ptr);

        // 揭开隐身
        let _: () = objc2::msg_send![ns_window, setIgnoresMouseEvents: false];
        ns_window.setAlphaValue(1.0);
        let _: () = objc2::msg_send![ns_window, makeKeyAndOrderFront: std::ptr::null::<objc2::runtime::AnyObject>()];

        // activate_app：让 Voidnix 成为 active app（LSUIElement 首次点击问题）
        crate::macos::mac_utils::activate_app();

        // 鼠标位置
        let mouse_loc = NSEvent::mouseLocation();
        let mouse_x = mouse_loc.x;
        let mouse_y = screen_height - mouse_loc.y;

        let mut d = data.clone();
        d.mouse_x = mouse_x;
        d.mouse_y = mouse_y;
        // picker_jpeg_path 供前端放大镜异步加载
        d.data_url = picker_jpeg_path().to_string_lossy().to_string();

        if let Ok(json) = serde_json::to_string(&d) {
            let _ = window.eval(&format!(
                "window.__screenshotData = {}; window.dispatchEvent(new CustomEvent('__screenshot_ready'));",
                json
            ));
        }

        // 背景已通过 CALayer 显示，现在在主线程同步生成 picker JPEG（放大镜用）。
        // NSBitmapImageRep initWithCGImage: 必须在主线程调用，后台线程会静默失败。
        // 此时背景已在屏幕上，这里多花 ~40ms 用户感知不到。
        let cg_ptr = get_cg_image();
        if !cg_ptr.is_null() {
            prepare_picker_jpeg(cg_ptr);
        }
    }
    start_mouse_tracker(app);
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn enter_impl(_app: &tauri::AppHandle, _data: &ScreenshotData) -> Result<(), String> {
    Err("仅支持 macOS".to_string())
}

/// activate_app 触发点（前端 onBgLoaded 调用，现在是 no-op，保留兼容）
#[tauri::command]
pub async fn show_screenshot_window(_app: tauri::AppHandle) -> Result<(), String> {
    Ok(())
}

/// 退出截屏模式。no_restore_focus 为 true 时不恢复前台应用焦点（OCR 场景）。
#[tauri::command]
pub async fn exit_screenshot_mode(
    app: tauri::AppHandle,
    no_restore_focus: Option<bool>,
) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
    let app_c = app.clone();
    app.run_on_main_thread(move || { let _ = tx.send(exit_impl(&app_c, no_restore_focus.unwrap_or(false))); })
        .map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())?
}

#[cfg(target_os = "macos")]
fn exit_impl(app: &tauri::AppHandle, no_restore_focus: bool) -> Result<(), String> {
    use tauri::Manager;
    use objc2_app_kit::{NSApplicationActivationOptions, NSWindow, NSWindowAnimationBehavior, NSWorkspace};

    stop_mouse_tracker();

    let window = app.get_webview_window("screenshot").ok_or("找不到截图窗口")?;
    let raw = window.ns_window().map_err(|e| e.to_string())?.cast::<NSWindow>();
    unsafe {
        let ns_window: &NSWindow = raw.as_ref().ok_or("NSWindow 为空")?;
        ns_window.setAnimationBehavior(NSWindowAnimationBehavior::None);
        ns_window.setAlphaValue(0.0);
        let _: () = objc2::msg_send![ns_window, setIgnoresMouseEvents: true];
        let _: () = objc2::msg_send![ns_window, resignKeyWindow];
        // 清除 CALayer 背景，释放显存引用
        let ptr = raw.cast::<NSWindow>() as *mut std::ffi::c_void;
        voidnix_screenshot_clear_background(ptr);
    }

    let prev_pid = PREV_FRONT_PID.swap(0, Ordering::SeqCst);
    if !no_restore_focus && prev_pid > 0 {
        let ws = NSWorkspace::sharedWorkspace();
        if let Some(target) = ws.runningApplications().iter()
            .find(|a| a.processIdentifier() as i32 == prev_pid)
        {
            #[allow(deprecated)]
            target.activateWithOptions(NSApplicationActivationOptions::ActivateIgnoringOtherApps);
        }
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn exit_impl(_app: &tauri::AppHandle, _no_restore_focus: bool) -> Result<(), String> { Ok(()) }

// ── reactivate / mouse_tracker（不变）────────────────────────────────────────

#[cfg(target_os = "macos")]
pub fn reactivate_screenshot_window(app: &tauri::AppHandle) {
    use tauri::Manager;
    use objc2_app_kit::NSWindow;
    let Some(window) = app.get_webview_window("screenshot") else { return };
    let Ok(raw) = window.ns_window().map(|p| p.cast::<NSWindow>()) else { return };
    let Some(ns_window) = (unsafe { raw.as_ref() }) else { return };
    if ns_window.alphaValue() < 0.5 { return; }
    let is_on_active_space: bool = unsafe { objc2::msg_send![ns_window, isOnActiveSpace] };
    if !is_on_active_space { return; }
    unsafe {
        let _: () = objc2::msg_send![ns_window, makeKeyAndOrderFront: std::ptr::null::<objc2::runtime::AnyObject>()];
    }
    crate::macos::mac_utils::activate_app();
    let _ = window.eval("window.dispatchEvent(new Event('focus'))");
}

#[cfg(target_os = "macos")]
mod mouse_tracker {
    use std::sync::Mutex;
    use objc2::runtime::AnyObject;
    use tauri::Manager;

    struct SendObj(*mut AnyObject);
    unsafe impl Send for SendObj {}
    unsafe impl Sync for SendObj {}

    static GLOBAL_MONITOR: Mutex<SendObj> = Mutex::new(SendObj(std::ptr::null_mut()));
    static LOCAL_MONITOR: Mutex<SendObj> = Mutex::new(SendObj(std::ptr::null_mut()));

    pub fn start(app: &tauri::AppHandle) {
        use objc2::ClassType;
        use objc2_app_kit::{NSEvent, NSScreen};
        use objc2_foundation::MainThreadMarker;
        { let g = GLOBAL_MONITOR.lock().unwrap(); if !g.0.is_null() { return; } }
        let screen_h = {
            let mtm = match MainThreadMarker::new() { Some(m) => m, None => return };
            NSScreen::mainScreen(mtm).map(|s| s.frame().size.height).unwrap_or(0.0)
        };
        let mask: u64 = (1u64 << 5) | (1u64 << 6) | (1u64 << 7) | (1u64 << 27);
        {
            let app = app.clone();
            let blk = block2::RcBlock::new(move |_event: *mut AnyObject| { emit_mouse(&app, screen_h); });
            unsafe {
                let m: *mut AnyObject = objc2::msg_send![NSEvent::class(), addGlobalMonitorForEventsMatchingMask: mask, handler: &*blk];
                if !m.is_null() { let _: () = objc2::msg_send![m, retain]; *GLOBAL_MONITOR.lock().unwrap() = SendObj(m); std::mem::forget(blk); }
            }
        }
        {
            let app = app.clone();
            let blk = block2::RcBlock::new(move |event: *mut AnyObject| -> *mut AnyObject { emit_mouse(&app, screen_h); event });
            unsafe {
                let m: *mut AnyObject = objc2::msg_send![NSEvent::class(), addLocalMonitorForEventsMatchingMask: mask, handler: &*blk];
                if !m.is_null() { let _: () = objc2::msg_send![m, retain]; *LOCAL_MONITOR.lock().unwrap() = SendObj(m); std::mem::forget(blk); }
            }
        }
    }

    pub fn stop() {
        use objc2::ClassType;
        use objc2_app_kit::NSEvent;
        for slot in [&GLOBAL_MONITOR, &LOCAL_MONITOR] {
            let mut g = slot.lock().unwrap();
            if !g.0.is_null() {
                unsafe { let _: () = objc2::msg_send![NSEvent::class(), removeMonitor: g.0]; let _: () = objc2::msg_send![g.0, release]; }
                *g = SendObj(std::ptr::null_mut());
            }
        }
    }

    fn emit_mouse(app: &tauri::AppHandle, screen_h: f64) {
        use objc2_app_kit::{NSEvent, NSWindow};
        let Some(window) = app.get_webview_window("screenshot") else { return };
        let loc = NSEvent::mouseLocation();
        let cx = loc.x; let cy = screen_h - loc.y;
        let _ = window.eval(&format!("window.__setScreenshotCross && window.__setScreenshotCross({},{})", cx, cy));
        if let Ok(raw) = window.ns_window().map(|p| p.cast::<NSWindow>()) {
            unsafe {
                if let Some(ns) = raw.as_ref() {
                    let on_space: bool = objc2::msg_send![ns, isOnActiveSpace];
                    if ns.alphaValue() > 0.5 && !ns.isKeyWindow() && on_space {
                        let _: () = objc2::msg_send![ns, makeKeyAndOrderFront: std::ptr::null::<AnyObject>()];
                        crate::macos::mac_utils::activate_app();
                    }
                }
            }
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn start_mouse_tracker(app: &tauri::AppHandle) { mouse_tracker::start(app); }
#[cfg(target_os = "macos")]
pub(crate) fn stop_mouse_tracker() { mouse_tracker::stop(); }
#[cfg(not(target_os = "macos"))]
pub(crate) fn start_mouse_tracker(_app: &tauri::AppHandle) {}
#[cfg(not(target_os = "macos"))]
pub(crate) fn stop_mouse_tracker() {}

// ── OCR 窗口 ──────────────────────────────────────────────────────────────────

/// 退出截屏模式，打开 OCR 窗口并传入选区数据。
#[tauri::command]
pub async fn open_ocr_window(
    app: tauri::AppHandle,
    sel_x: f64, sel_y: f64, sel_w: f64, sel_h: f64,
    scale: f64,
    annotation_png: String,
) -> Result<(), String> {
    use tauri::Manager;

    // 先生成选区 PNG 保存到临时文件，供 OCR 窗口预览
    let ann = if annotation_png.is_empty() { None } else { Some(decode_image_data(&annotation_png)?) };

    #[cfg(target_os = "macos")]
    let image_path = {
        let png = crop_with_annotation(sel_x, sel_y, sel_w, sel_h, scale, ann.as_deref())?;
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis();
        let path = std::env::temp_dir().join(format!("voidnix_ocr_preview_{}.png", ts));
        std::fs::write(&path, &png).map_err(|e| e.to_string())?;
        path.to_string_lossy().to_string()
    };
    #[cfg(not(target_os = "macos"))]
    let image_path = String::new();

    let ocr_data = serde_json::json!({
        "image_path": image_path,
        "sel_x": sel_x,
        "sel_y": sel_y,
        "sel_w": sel_w,
        "sel_h": sel_h,
        "scale": scale,
        "annotation_png": annotation_png,
    });

    let window = app.get_webview_window("ocr").ok_or("找不到 OCR 窗口")?;

    // 注入数据并触发事件
    let json = serde_json::to_string(&ocr_data).map_err(|e| e.to_string())?;
    window.eval(&format!(
        "window.__ocrData = {}; window.dispatchEvent(new CustomEvent('__ocr_ready'));",
        json
    )).map_err(|e| e.to_string())?;

    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;

    Ok(())
}

// ── 钉图 ──────────────────────────────────────────────────────────────────────

/// 将选区截图钉在屏幕上（用 Tauri webview 创建独立窗口，支持移动/缩放/透明度/关闭）。
#[tauri::command]
pub async fn pin_image(
    app: tauri::AppHandle,
    sel_x: f64, sel_y: f64, sel_w: f64, sel_h: f64,
    scale: f64,
    annotation_png: String,
) -> Result<(), String> {
    let ann = if annotation_png.is_empty() { None } else { Some(decode_image_data(&annotation_png)?) };

    #[cfg(target_os = "macos")]
    {
        let png = crop_with_annotation(sel_x, sel_y, sel_w, sel_h, scale, ann.as_deref())?;
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis();
        let path = std::env::temp_dir().join(format!("voidnix_pin_{}.png", ts));
        std::fs::write(&path, &png).map_err(|e| e.to_string())?;

        let path_str = path.to_string_lossy().to_string();
        let win_w = sel_w;
        let win_h = sel_h;
        // 钉图窗口与截屏选区同位置（前端逻辑像素，左上原点）
        let win_x = sel_x;
        let win_y = sel_y;
        let label = format!("pin-{}", ts);

        // 主线程创建 Tauri webview 窗口
        let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let app_clone = app.clone();
        app.run_on_main_thread(move || {
            let r = create_pin_webview(&app_clone, &label, &path_str, win_w, win_h, win_x, win_y);
            let _ = tx.send(r);
        }).map_err(|e| e.to_string())?;
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
) -> Result<(), String> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};
    use objc2_app_kit::{NSWindow, NSWindowCollectionBehavior};

    // URL 把图片路径作为 query 参数传给 PinView
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
        .visible(true)  // 直接显示，不等待内容加载
        .accept_first_mouse(true);  // 立即接受鼠标事件

    let window = builder.build().map_err(|e| e.to_string())?;

    // 应用 macOS 专属配置：圆角、不随 app 失活隐藏、跨 Space 可见
    if let Ok(raw) = window.ns_window().map(|p| p.cast::<NSWindow>()) {
        unsafe {
            if let Some(ns) = raw.as_ref() {
                // 不随 app 失活隐藏（核心修复：钉图独立于 Voidnix 激活状态）
                let _: () = objc2::msg_send![ns, setHidesOnDeactivate: false];
                // 浮动层级：高于普通窗口
                ns.setLevel(objc2_app_kit::NSFloatingWindowLevel as isize);
                // 在所有桌面可见 + 可覆盖全屏 + 不出现在 Mission Control
                let behavior = NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::FullScreenAuxiliary
                    | NSWindowCollectionBehavior::Stationary;
                ns.setCollectionBehavior(behavior);
                // 圆角窗口：contentView.layer.cornerRadius
                if let Some(content_view) = ns.contentView() {
                    let _: () = objc2::msg_send![&content_view, setWantsLayer: true];
                    let layer: *mut objc2::runtime::AnyObject = objc2::msg_send![&content_view, layer];
                    if !layer.is_null() {
                        let _: () = objc2::msg_send![layer, setCornerRadius: 10.0_f64];
                        let _: () = objc2::msg_send![layer, setMasksToBounds: true];
                    }
                }
            }
        }
    }

    // 窗口已经设置为 visible(true)，不需要再调用 show
    Ok(())
}

/// 设置当前钉图窗口透明度
#[tauri::command]
pub async fn set_pin_window_opacity(window: tauri::WebviewWindow, opacity: f64) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::NSWindow;
        use tauri::Manager;
        let app_handle = window.app_handle().clone();
        let opacity_val = opacity.clamp(0.1, 1.0);
        // 在主线程获取 ns_window 并设置透明度，所有 NSWindow 操作必须在主线程
        app_handle.run_on_main_thread(move || {
            if let Ok(raw) = window.ns_window().map(|p| p.cast::<NSWindow>()) {
                unsafe {
                    if let Some(ns) = raw.as_ref() {
                        ns.setAlphaValue(opacity_val);
                    }
                }
            }
        }).map_err(|e| e.to_string())?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, opacity);
    }
    Ok(())
}

// ── 通用模块面板触发 ──────────────────────────────────────────────────────────

/// 显示主窗口并向指定模块发送打开面板事件。
/// 模块通过前端 `onOpenPanel` 回调接收 payload。
#[tauri::command]
pub async fn open_module_panel(
    app: tauri::AppHandle,
    module_id: String,
    payload: serde_json::Value,
) -> Result<(), String> {
    use tauri::Emitter;

    let event_payload = serde_json::json!({
        "moduleId": module_id,
        "payload": payload,
    });

    let app_handle = app.clone();
    app.run_on_main_thread(move || {
        crate::macos::webkit_tuning::show_main(&app_handle);
        let _ = app_handle.emit("open-module-panel", event_payload);
    }).map_err(|e| e.to_string())?;

    Ok(())
}


pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new("screenshot")
        .setup(|_app, _api| {
            #[cfg(target_os = "macos")]
            {
                // 快捷键钩子：在快捷键按下后、窗口显示前完成截屏全流程
                use tauri::Emitter;
                crate::core::shortcut::register_shortcut_hook("screenshot", Box::new(|app, _ctx| {
                    let app_clone = app.clone();
                    std::thread::spawn(move || {
                        let result = capture_screen();
                        match result {
                            Ok(data) => {
                                let app_for_enter = app_clone.clone();
                                let _ = app_clone.run_on_main_thread(move || {
                                    enter_screenshot_mode_sync(&app_for_enter, data);
                                });
                            }
                            Err(e) => {
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
