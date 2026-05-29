use base64::Engine;
use serde::{Deserialize, Serialize};
use std::process::Command;
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicI32, Ordering};

/// 截屏前的前台应用 PID，退出截屏时用于恢复焦点。
#[cfg(target_os = "macos")]
static PREV_FRONT_PID: AtomicI32 = AtomicI32::new(0);

/// 截屏会话代次。enter 时自增；exit 的 fade 完成回调里若代次已变，
/// 说明在 fade 过程中又触发了新一次截屏，此时不能清空 CALayer 背景，
/// 否则会把新截屏的背景一起抹掉。
#[cfg(target_os = "macos")]
static SCREENSHOT_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// 截屏会话占用锁。enter 期间为 true，hook 据此屏蔽重复触发，
/// 避免连按快捷键叠加多次 capture + 多帧 Operation.vue 重 mount。
/// exit_impl 主线程入口处释放（fade 仅视觉，逻辑会话已结束）。
#[cfg(target_os = "macos")]
static IS_IN_SCREENSHOT_SESSION: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

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

/// 文本检测返回的单个文本行边界（CSS 像素，左上原点）。
#[derive(Serialize, Deserialize, Clone, Debug)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TextRegion {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
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
    fn voidnix_screenshot_install_scroll_mask(
        ns_window_ptr: *mut std::ffi::c_void,
        sel_x: f64, sel_y: f64, sel_w: f64, sel_h: f64,
    ) -> bool;
    fn voidnix_screenshot_remove_scroll_mask(ns_window_ptr: *mut std::ffi::c_void);
    fn voidnix_screenshot_window_number(ns_window_ptr: *mut std::ffi::c_void) -> std::os::raw::c_long;
    fn voidnix_screenshot_set_ignores_mouse(ns_window_ptr: *mut std::ffi::c_void, ignores: i32);
    fn voidnix_screenshot_get_mouse_location(out_x: *mut f64, out_y: *mut f64, screen_height: f64);
    fn voidnix_screenshot_capture_region(
        ns_window_ptr: *mut std::ffi::c_void,
        sel_x: f64, sel_y: f64, sel_w: f64, sel_h: f64,
        out_image: *mut *mut std::ffi::c_void,
    );
    /// sharing=0 → NSWindowSharingNone（截屏 API 永不合成此窗口）
    /// sharing=1 → NSWindowSharingReadOnly（恢复默认）
    fn voidnix_screenshot_set_sharing(ns_window_ptr: *mut std::ffi::c_void, sharing: i32);
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

/// 把 PNG 字节解码成 CGImage（保留 +1 引用，调用方负责 release）。
/// 钉图路径用：从 crop_with_annotation 的 PNG 重新解码，交给 CALayer 直贴渲染，
/// 不等 WebView 加载就能看到图像。
#[cfg(target_os = "macos")]
fn png_bytes_to_cgimage(png: &[u8]) -> *mut std::ffi::c_void {
    use objc2::runtime::AnyObject;
    if png.is_empty() { return std::ptr::null_mut(); }
    unsafe {
        let cls_data = objc2::class!(NSData);
        let ns_data: *mut AnyObject = objc2::msg_send![
            cls_data,
            dataWithBytes: png.as_ptr() as *const std::ffi::c_void,
            length: png.len()
        ];
        if ns_data.is_null() { return std::ptr::null_mut(); }
        let cls_rep = objc2::class!(NSBitmapImageRep);
        let rep: *mut AnyObject = objc2::msg_send![cls_rep, imageRepWithData: ns_data];
        if rep.is_null() { return std::ptr::null_mut(); }
        let cg: *mut std::ffi::c_void = objc2::msg_send![rep, CGImage];
        if cg.is_null() { return std::ptr::null_mut(); }
        CGImageRetain(cg)
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

/// 检测当前截屏中所有文本的紧致边界框（Apple Vision）。
/// 返回坐标为 CSS 像素、左上原点。
/// 实现：每个 VNRecognizedTextObservation 直接 emit 其 boundingBox（即一整行的包络框）。
/// 历史上曾按词边界（candidate.boundingBox(for:)）拆段以避免覆盖行内空白，但实测会出现：
///   - 行内多个 word box 高度不齐（如 "Hello" 与 "world"），上下边缘留缝
///   - 部分 word 的 boundingBox(for:) 返回 nil，导致整行内零散字符漏掉
/// 行内空格被一并模糊在视觉上无副作用（空白处模糊后仍是空白），换取"绝不漏行"。
#[tauri::command]
#[cfg_attr(feature = "specta", specta::specta)]
pub async fn detect_text_regions(scale: f64) -> Result<Vec<TextRegion>, String> {
    #[cfg(target_os = "macos")]
    {
        let path = picker_jpeg_path();
        if !path.exists() {
            return Err("无截屏数据".to_string());
        }
        let script = format!(
            r#"import Vision; import AppKit
let url = URL(fileURLWithPath: "{path}")
guard let img = NSImage(contentsOf: url), let cg = img.cgImage(forProposedRect: nil, context: nil, hints: nil) else {{ exit(0) }}
let imgW = Double(cg.width), imgH = Double(cg.height)
let scale: Double = {scale}
let req = VNRecognizeTextRequest()
req.recognitionLevel = .accurate
req.usesLanguageCorrection = false
req.recognitionLanguages = ["zh-Hans","zh-Hant","en-US","ja"]
try? VNImageRequestHandler(cgImage: cg, options: [:]).perform([req])
func emit(_ rect: CGRect) {{
  let xPx = rect.origin.x * imgW
  let wPx = rect.size.width * imgW
  let hPx = rect.size.height * imgH
  let yPxBottom = rect.origin.y * imgH
  let yPxTop = imgH - yPxBottom - hPx
  print("\(xPx/scale),\(yPxTop/scale),\(wPx/scale),\(hPx/scale)")
}}
for obs in (req.results ?? []) {{
  emit(obs.boundingBox)
}}"#,
            path = path.display(),
            scale = scale,
        );
        let out = Command::new("swift").args(["-e", &script]).output()
            .map_err(|e| format!("swift 失败: {}", e))?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
        }
        let stdout = String::from_utf8_lossy(&out.stdout);
        let mut regions = Vec::new();
        for line in stdout.lines() {
            let line = line.trim();
            if line.is_empty() { continue; }
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() != 4 { continue; }
            let (Ok(x), Ok(y), Ok(w), Ok(h)) = (
                parts[0].parse::<f64>(),
                parts[1].parse::<f64>(),
                parts[2].parse::<f64>(),
                parts[3].parse::<f64>(),
            ) else { continue };
            regions.push(TextRegion { x, y, w, h });
        }
        Ok(regions)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = scale;
        Err("仅支持 macOS".to_string())
    }
}
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

// ── 窗口淡入/淡出 ─────────────────────────────────────────────────────────────

/// 用 contentView.layer.opacity（CABasicAnimation）做淡入/淡出。
/// 关键：NSWindow.alphaValue 保持 1.0，否则 alpha=0 时 macOS 不进行命中测试，
/// 用户没法拖拽选区。`ns_window_addr` 是 NSWindow 指针的 usize 副本。
#[cfg(target_os = "macos")]
fn fade_window_layer_opacity(
    ns_window_addr: usize,
    target: f32,
    duration: f64,
    completion: Option<Box<dyn FnOnce() + Send + 'static>>,
) {
    use objc2::runtime::AnyObject;
    use objc2_foundation::NSString;
    use std::sync::{Arc, Mutex};

    unsafe {
        let nsw = ns_window_addr as *mut AnyObject;
        let content_view: *mut AnyObject = objc2::msg_send![nsw, contentView];
        if content_view.is_null() {
            if let Some(cb) = completion { cb(); }
            return;
        }
        let _: () = objc2::msg_send![content_view, setWantsLayer: true];
        let layer: *mut AnyObject = objc2::msg_send![content_view, layer];
        if layer.is_null() {
            if let Some(cb) = completion { cb(); }
            return;
        }

        let from_opacity: f32 = objc2::msg_send![layer, opacity];
        let cls_anim = objc2::class!(CABasicAnimation);
        let key_path = NSString::from_str("opacity");
        let anim: *mut AnyObject = objc2::msg_send![cls_anim, animationWithKeyPath: &*key_path];

        let cls_num = objc2::class!(NSNumber);
        let from_val: *mut AnyObject = objc2::msg_send![cls_num, numberWithFloat: from_opacity];
        let to_val: *mut AnyObject = objc2::msg_send![cls_num, numberWithFloat: target];
        let _: () = objc2::msg_send![anim, setFromValue: from_val];
        let _: () = objc2::msg_send![anim, setToValue: to_val];
        let _: () = objc2::msg_send![anim, setDuration: duration];

        // 显式设置模型层的最终值，否则动画结束后会跳回 fromValue。
        let _: () = objc2::msg_send![layer, setOpacity: target];

        let anim_key = NSString::from_str("voidnix-fade-opacity");
        let cls_ct = objc2::class!(CATransaction);

        match completion {
            Some(cb) => {
                let slot: Arc<Mutex<Option<Box<dyn FnOnce() + Send + 'static>>>> =
                    Arc::new(Mutex::new(Some(cb)));
                let slot_clone = Arc::clone(&slot);
                let done = block2::RcBlock::new(move || {
                    if let Some(f) = slot_clone.lock().unwrap().take() {
                        f();
                    }
                });
                let _: () = objc2::msg_send![cls_ct, begin];
                let _: () = objc2::msg_send![cls_ct, setCompletionBlock: &*done];
                let _: () = objc2::msg_send![layer, addAnimation: anim, forKey: &*anim_key];
                let _: () = objc2::msg_send![cls_ct, commit];
            }
            None => {
                let _: () = objc2::msg_send![layer, addAnimation: anim, forKey: &*anim_key];
            }
        }
    }
}

/// 直接设置 contentView.layer.opacity（无动画）。
#[cfg(target_os = "macos")]
fn set_window_layer_opacity(ns_window_addr: usize, opacity: f32) {
    use objc2::runtime::AnyObject;
    unsafe {
        let nsw = ns_window_addr as *mut AnyObject;
        let content_view: *mut AnyObject = objc2::msg_send![nsw, contentView];
        if content_view.is_null() { return; }
        let _: () = objc2::msg_send![content_view, setWantsLayer: true];
        let layer: *mut AnyObject = objc2::msg_send![content_view, layer];
        if layer.is_null() { return; }
        // 关闭隐式动画，直接生效
        let cls_ct = objc2::class!(CATransaction);
        let _: () = objc2::msg_send![cls_ct, begin];
        let _: () = objc2::msg_send![cls_ct, setDisableActions: true];
        let _: () = objc2::msg_send![layer, setOpacity: opacity];
        let _: () = objc2::msg_send![cls_ct, commit];
    }
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

    // 进入新会话：让正在 fade out 的 exit 完成回调失效（避免清空新背景）
    SCREENSHOT_GEN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

    let self_pid = std::process::id() as i32;
    let prev_pid = NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .map(|a| a.processIdentifier())
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

        // 先把旧的 picker JPEG 删掉，再用当前帧生成新的（同步约 40ms）。
        // 必须在 eval 之前完成，否则前端 loadPickerImage 的轮询会先抓到上一次截屏的旧文件，
        // 放大镜里就是上次的画面。NSBitmapImageRep initWithCGImage 必须在主线程。
        let _ = std::fs::remove_file(picker_jpeg_path());
        if !cg_image_ptr.is_null() {
            prepare_picker_jpeg(cg_image_ptr);
        }

        // SkyLight 迁移到当前 active Space（用 SLSAddWindowsToSpaces，
        // 全屏 Space 和普通桌面都支持，跨 macOS 13/14/15 稳定）
        let window_number: objc2_foundation::NSInteger = objc2::msg_send![ns_window, windowNumber];
        let _ = crate::macos::skylight::move_window_to_active_space(window_number as i64, ns_window_ptr);

        // 待命态：layer.opacity=0（视觉不可见）+ alpha=1.0（保留命中测试）+
        // ignoresMouseEvents=false（事件本窗口接收，绝不击穿到底下应用——若让
        // Finder 等拿到 mousedown，implicit grab 后即便后续解锁也截不回这一次拖动）。
        // 视觉淡入推迟到 Operation.vue.onMounted → screenshot_overlay_ready 触发，
        // mount 前的 mousedown 即使没 handler 也只是"丢"，不会被外部 grab。
        let ns_window_addr = raw.cast::<NSWindow>() as usize;
        set_window_layer_opacity(ns_window_addr, 0.0);
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
            let _ = window.eval(format!(
                "window.__screenshotData = {}; window.dispatchEvent(new CustomEvent('__screenshot_ready'));",
                json
            ));
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

/// 截屏 overlay 前端就绪信号：在 Operation.vue.onMounted 里 invoke。
/// 此时 @mousedown 等监听已挂在 DOM 上，触发淡入让用户看到 UI。
/// 鼠标事件接收已在 enter_impl 里开启，这里只负责视觉上的解锁。
#[tauri::command]
pub async fn screenshot_overlay_ready(app: tauri::AppHandle) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
    let app_c = app.clone();
    app.run_on_main_thread(move || { let _ = tx.send(overlay_ready_impl(&app_c)); })
        .map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())?
}

#[cfg(target_os = "macos")]
fn overlay_ready_impl(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    use objc2_app_kit::NSWindow;
    let window = app.get_webview_window("screenshot").ok_or("找不到截图窗口")?;
    let raw = window.ns_window().map_err(|e| e.to_string())?.cast::<NSWindow>();
    let ns_window_addr = raw.cast::<NSWindow>() as usize;
    fade_window_layer_opacity(ns_window_addr, 1.0, 0.18, None);
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn overlay_ready_impl(_app: &tauri::AppHandle) -> Result<(), String> { Ok(()) }

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
    // 逻辑会话已结束（视觉淡出还在跑也无所谓），立即释放占用锁。
    // 后续快捷键能马上进入新会话；新 enter_impl 会自增 SCREENSHOT_GEN
    // 让旧 fade 的 completion 跳过清理，避免误擦新背景。
    IS_IN_SCREENSHOT_SESSION.store(false, std::sync::atomic::Ordering::SeqCst);

    let window = app.get_webview_window("screenshot").ok_or("找不到截图窗口")?;
    let raw = window.ns_window().map_err(|e| e.to_string())?.cast::<NSWindow>();
    let session_gen = SCREENSHOT_GEN.load(std::sync::atomic::Ordering::SeqCst);
    let ns_window_addr = raw.cast::<NSWindow>() as usize;
    unsafe {
        let ns_window: &NSWindow = raw.as_ref().ok_or("NSWindow 为空")?;
        ns_window.setAnimationBehavior(NSWindowAnimationBehavior::None);
        // 立即停止接收鼠标事件并交出 key 状态，UI 视觉上仍在淡出
        let _: () = objc2::msg_send![ns_window, setIgnoresMouseEvents: true];
        let _: () = objc2::msg_send![ns_window, resignKeyWindow];
    }

    // 淡出：layer.opacity → 0，动画结束后清理 CALayer 背景（释放显存），
    // 同时把 NSWindow.alpha 设回 0 让窗口彻底休眠。
    // 若期间又触发了新一次截屏（代次变化），跳过收尾以免抹掉新背景。
    fade_window_layer_opacity(ns_window_addr, 0.0, 0.15, Some(Box::new(move || {
        if SCREENSHOT_GEN.load(std::sync::atomic::Ordering::SeqCst) == session_gen {
            unsafe {
                let ptr = ns_window_addr as *mut std::ffi::c_void;
                voidnix_screenshot_clear_background(ptr);
                let nsw = ns_window_addr as *mut objc2::runtime::AnyObject;
                let _: () = objc2::msg_send![nsw, setAlphaValue: 0.0_f64];
            }
        }
    })));

    let prev_pid = PREV_FRONT_PID.swap(0, Ordering::SeqCst);
    if !no_restore_focus && prev_pid > 0 {
        let ws = NSWorkspace::sharedWorkspace();
        if let Some(target) = ws.runningApplications().iter()
            .find(|a| a.processIdentifier() == prev_pid)
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
        let _ = window.eval(format!("window.__setScreenshotCross && window.__setScreenshotCross({},{})", cx, cy));
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
    window.eval(format!(
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

        // 同步把 PNG 解码成 CGImage：钉图窗口用 CALayer 直接呈现，瞬时可见，
        // 不等 WebView 加载完毕。raw 指针跨线程靠 usize 中转。
        let cg_addr = png_bytes_to_cgimage(&png) as usize;

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
            let cg_ptr = cg_addr as *mut std::ffi::c_void;
            let r = create_pin_webview(&app_clone, &label, &path_str, win_w, win_h, win_x, win_y, cg_ptr);
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
    cg_image: *mut std::ffi::c_void,
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
                // 提到 status 层级：与截屏窗口同级，新窗口排在前面，
                // 这样截屏 fade-out 还没完成时钉图就能立刻可见而不被截屏盖住。
                ns.setLevel(objc2_app_kit::NSStatusWindowLevel);
                // 在所有桌面可见 + 可覆盖全屏 + 不出现在 Mission Control
                let behavior = NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::FullScreenAuxiliary
                    | NSWindowCollectionBehavior::Stationary;
                ns.setCollectionBehavior(behavior);
                // 缩放时锁定内容长宽比为原始选区比例（macOS 自动约束 resize）
                ns.setContentAspectRatio(objc2_foundation::NSSize::new(width, height));
                // 圆角窗口：contentView.layer.cornerRadius
                if let Some(content_view) = ns.contentView() {
                    let _: () = objc2::msg_send![&content_view, setWantsLayer: true];
                    let layer: *mut objc2::runtime::AnyObject = objc2::msg_send![&content_view, layer];
                    if !layer.is_null() {
                        let _: () = objc2::msg_send![layer, setCornerRadius: 10.0_f64];
                        let _: () = objc2::msg_send![layer, setMasksToBounds: true];
                    }
                }

                // 钉图核心：把图像作为 CALayer 直接贴在 contentView 下，瞬时呈现。
                // 不等 WebView 把 HTML/Vue/<img> 加载完毕（那条链路要 100-150ms），
                // 钉图按钮按下的瞬间就能看到图。复用 screenshot_overlay.mm 的实现。
                let ns_window_void = raw.cast::<NSWindow>() as *mut std::ffi::c_void;
                if !cg_image.is_null() {
                    voidnix_screenshot_install_background_layer(ns_window_void);
                    voidnix_screenshot_set_background(ns_window_void, cg_image);
                    // CALayer.setContents 自身会 retain，这里释放 png_bytes_to_cgimage 加上的引用。
                    CGImageRelease(cg_image);
                }

                // 显式抬起为 key 并激活 app，确保钉图窗口出现时立即聚焦
                // （builder.focused(true) 在某些情形下不够确定）
                let _: () = objc2::msg_send![ns, makeKeyAndOrderFront: std::ptr::null::<objc2::runtime::AnyObject>()];
            }
        }
    }
    crate::macos::mac_utils::activate_app();

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


// ── 滚动截屏 ──────────────────────────────────────────────────────────────────
//
// 工业级方案要点：
//   1. 透明放行：装"暗化遮罩 + 选区挖洞"的 CAShapeLayer，alpha=0 像素让 macOS
//      把鼠标事件路由给底下窗口；用户在选区内自由滚动，palette/preview 仍可点。
//   2. 实时抓取：CGWindowListCreateImage(rect, .OnScreenBelowWindow, screenshotWin)
//      获取选区区域底下窗口的实时画面，零编码、零拷贝。
//   3. 帧间对齐：把上一帧底部条带与新帧逐行做 64-bit FNV 哈希，先精配（哈希等）
//      再容差精配（SAD 子像素），找到最大重合行偏移 → 拼接增量。
//   4. 终态合成：累积 RGBA 缓冲一直增长（仅 append 行），读出时编码 PNG/JPEG。
//
// 整体延迟：抓帧 ~5ms + stitch ~3ms + emit base64 缩略 ~2ms → 30fps 余裕。

#[cfg(target_os = "macos")]
mod scroll_capture {
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Mutex;
    use tauri::Emitter;

    /// 滚动会话状态。一份全局，确保不会出现多个累积缓冲并行。
    pub struct ScrollSession {
        /// 选区在屏幕上的逻辑位置（CSS 像素，左上原点）。
        pub sel_x: f64,
        pub sel_y: f64,
        pub sel_w: f64,
        pub sel_h: f64,
        /// 物理像素缩放（一般 = backingScaleFactor）。当前未直接使用，
        /// 保留供未来"基于物理像素 stitch 阈值动态缩放"扩展。
        #[allow(dead_code)]
        pub scale: f64,
        /// 物理像素宽高。row_bytes = pw * 4。
        pub pw: usize,
        pub ph_per_frame: usize,
        /// 累积缓冲：BGRA8888，每行 row_bytes 字节，行序自上而下。
        /// 长度 = total_rows * row_bytes。
        pub buf: Vec<u8>,
        /// 当前已累积的行数。
        pub total_rows: usize,
        /// 上一帧的全部 RGBA（用于与新帧 stitch）。
        pub prev_frame: Vec<u8>,
        /// 截屏窗口的 CGWindowID（CGWindowListCreateImage 需要排除自身）。
        pub overlay_window_id: u32,
        /// 截屏窗口指针（usize 化）；ignoresMouseEvents 切换需要。
        pub ns_window_addr: usize,
        /// 当前 ignoresMouseEvents 状态，避免重复设。
        pub ignoring_mouse: bool,
        /// 用 emit 推给前端的预览帧序号。
        pub emit_seq: u64,
    }

    impl ScrollSession {
        pub fn row_bytes(&self) -> usize {
            self.pw * 4
        }
    }

    /// 全局会话状态。Mutex 保证读写互斥；AtomicBool 用于运行循环 fast-check 退出。
    pub static SESSION: Mutex<Option<ScrollSession>> = Mutex::new(None);
    pub static IS_RUNNING: AtomicBool = AtomicBool::new(false);
    /// 节流推送给前端的纳秒时间戳，最多 30fps。
    pub static LAST_EMIT_NS: AtomicU64 = AtomicU64::new(0);

    /// 把 CGImage 转换成连续 BGRA8 字节（无 padding）。失败返回 None。
    /// 不依赖 CGImage 内部 bytesPerRow（可能含 padding），用 CGContext 重绘到自有缓冲。
    pub fn cg_image_to_bgra8(cg: *mut std::ffi::c_void) -> Option<(usize, usize, Vec<u8>)> {
        if cg.is_null() {
            return None;
        }
        unsafe {
            extern "C" {
                fn CGImageGetWidth(image: *mut std::ffi::c_void) -> usize;
                fn CGImageGetHeight(image: *mut std::ffi::c_void) -> usize;
                fn CGColorSpaceCreateDeviceRGB() -> *mut std::ffi::c_void;
                fn CGColorSpaceRelease(cs: *mut std::ffi::c_void);
                fn CGBitmapContextCreate(
                    data: *mut std::ffi::c_void,
                    width: usize,
                    height: usize,
                    bits_per_component: usize,
                    bytes_per_row: usize,
                    space: *mut std::ffi::c_void,
                    bitmap_info: u32,
                ) -> *mut std::ffi::c_void;
                fn CGContextDrawImage(
                    ctx: *mut std::ffi::c_void,
                    rect: core_graphics::geometry::CGRect,
                    image: *mut std::ffi::c_void,
                );
                fn CGContextRelease(ctx: *mut std::ffi::c_void);
            }
            let w = CGImageGetWidth(cg);
            let h = CGImageGetHeight(cg);
            if w == 0 || h == 0 {
                return None;
            }
            let row_bytes = w * 4;
            let mut buf = vec![0u8; row_bytes * h];
            let cs = CGColorSpaceCreateDeviceRGB();
            // kCGImageAlphaPremultipliedFirst | kCGBitmapByteOrder32Little = 0x00002002 | 1 = (1<<13) | 2
            // 直接用 BGRA8888 little-endian: kCGImageAlphaPremultipliedFirst (2) | kCGBitmapByteOrder32Little (2 << 12)
            let bitmap_info: u32 = 2 | (2 << 12);
            let ctx = CGBitmapContextCreate(
                buf.as_mut_ptr() as *mut std::ffi::c_void,
                w,
                h,
                8,
                row_bytes,
                cs,
                bitmap_info,
            );
            if ctx.is_null() {
                CGColorSpaceRelease(cs);
                return None;
            }
            let rect = core_graphics::geometry::CGRect {
                origin: core_graphics::geometry::CGPoint { x: 0.0, y: 0.0 },
                size: core_graphics::geometry::CGSize { width: w as f64, height: h as f64 },
            };
            CGContextDrawImage(ctx, rect, cg);
            CGContextRelease(ctx);
            CGColorSpaceRelease(cs);
            Some((w, h, buf))
        }
    }

    /// 抓取选区在屏幕上对应的底下应用实时画面。
    /// 用 voidnix_screenshot_capture_region（ObjC 桥）枚举所有 on-screen 窗口，
    /// 排除 overlay 窗口，用 CGWindowListCreateImageFromArray 合成。
    /// 全程不 hide/show 窗口，零闪烁、零残影，线程安全。
    pub fn capture_below_overlay(
        sel_x: f64, sel_y: f64, sel_w: f64, sel_h: f64,
        _overlay_window_id: u32,
    ) -> Option<(usize, usize, Vec<u8>)> {
        unsafe {
            extern "C" {
                fn CGImageRelease(image: *mut std::ffi::c_void);
            }
            let ns_addr = {
                let g = SESSION.lock().unwrap();
                g.as_ref().map(|s| s.ns_window_addr).unwrap_or(0)
            };
            if ns_addr == 0 { return None; }

            let mut out_image: *mut std::ffi::c_void = std::ptr::null_mut();
            super::voidnix_screenshot_capture_region(
                ns_addr as *mut std::ffi::c_void,
                sel_x, sel_y, sel_w, sel_h,
                &mut out_image,
            );
            if out_image.is_null() { return None; }
            let res = cg_image_to_bgra8(out_image);
            CGImageRelease(out_image);
            res
        }
    }

    /// 把 RGBA 缓冲编码成 PNG。使用 CGImageDestination（线程安全）。
    pub fn encode_png(buf: &[u8], width: usize, height: usize) -> Result<Vec<u8>, String> {
        encode_image_with_cg(buf, width, height, "public.png", 1.0)
    }

    /// 把 RGBA 缓冲编码成 JPEG，用于实时预览推送（带宽小、解码快）。
    pub fn encode_jpeg(buf: &[u8], width: usize, height: usize, quality: f64) -> Result<Vec<u8>, String> {
        encode_image_with_cg(buf, width, height, "public.jpeg", quality)
    }

    /// 用 CGImageDestination 把 BGRA 缓冲编码成指定格式的图像字节。
    /// 全 Core Graphics 路径，线程安全（不依赖 AppKit），可在抓帧线程直接调。
    fn encode_image_with_cg(
        buf: &[u8],
        width: usize,
        height: usize,
        uti: &str,
        quality: f64,
    ) -> Result<Vec<u8>, String> {
        if buf.len() != width * height * 4 {
            return Err("buffer 大小不匹配".to_string());
        }
        unsafe {
            extern "C" {
                fn CGColorSpaceCreateDeviceRGB() -> *mut std::ffi::c_void;
                fn CGColorSpaceRelease(cs: *mut std::ffi::c_void);
                fn CGDataProviderCreateWithData(
                    info: *mut std::ffi::c_void,
                    data: *const u8,
                    size: usize,
                    release: *mut std::ffi::c_void,
                ) -> *mut std::ffi::c_void;
                fn CGDataProviderRelease(p: *mut std::ffi::c_void);
                fn CGImageCreate(
                    width: usize,
                    height: usize,
                    bits_per_component: usize,
                    bits_per_pixel: usize,
                    bytes_per_row: usize,
                    space: *mut std::ffi::c_void,
                    bitmap_info: u32,
                    provider: *mut std::ffi::c_void,
                    decode: *const f64,
                    should_interpolate: bool,
                    intent: u32,
                ) -> *mut std::ffi::c_void;
                fn CGImageRelease(image: *mut std::ffi::c_void);
                fn CFDataCreateMutable(allocator: *mut std::ffi::c_void, capacity: isize) -> *mut std::ffi::c_void;
                fn CFRelease(p: *mut std::ffi::c_void);
                fn CFStringCreateWithCString(
                    allocator: *mut std::ffi::c_void,
                    cstr: *const std::os::raw::c_char,
                    encoding: u32,
                ) -> *mut std::ffi::c_void;
                fn CGImageDestinationCreateWithData(
                    data: *mut std::ffi::c_void,
                    uti: *const std::ffi::c_void,
                    count: usize,
                    options: *mut std::ffi::c_void,
                ) -> *mut std::ffi::c_void;
                fn CGImageDestinationAddImage(
                    dst: *mut std::ffi::c_void,
                    image: *mut std::ffi::c_void,
                    properties: *mut std::ffi::c_void,
                );
                fn CGImageDestinationFinalize(dst: *mut std::ffi::c_void) -> bool;
                fn CFDataGetLength(data: *mut std::ffi::c_void) -> isize;
                fn CFDataGetBytePtr(data: *mut std::ffi::c_void) -> *const u8;
                fn CFNumberCreate(
                    allocator: *mut std::ffi::c_void,
                    number_type: i32,
                    value: *const std::ffi::c_void,
                ) -> *mut std::ffi::c_void;
                fn CFDictionaryCreate(
                    allocator: *mut std::ffi::c_void,
                    keys: *const *const std::ffi::c_void,
                    values: *const *const std::ffi::c_void,
                    num_values: isize,
                    key_callbacks: *const std::ffi::c_void,
                    value_callbacks: *const std::ffi::c_void,
                ) -> *mut std::ffi::c_void;
                static kCFTypeDictionaryKeyCallBacks: std::ffi::c_void;
                static kCFTypeDictionaryValueCallBacks: std::ffi::c_void;
            }

            // 1. CGImage from BGRA
            let cs = CGColorSpaceCreateDeviceRGB();
            // bitmapInfo: kCGImageAlphaPremultipliedFirst (2) | kCGBitmapByteOrder32Little (2 << 12)
            let bitmap_info: u32 = 2 | (2 << 12);
            let provider = CGDataProviderCreateWithData(
                std::ptr::null_mut(),
                buf.as_ptr(),
                buf.len(),
                std::ptr::null_mut(),
            );
            if provider.is_null() {
                CGColorSpaceRelease(cs);
                return Err("CGDataProvider 创建失败".to_string());
            }
            // kCGRenderingIntentDefault = 0
            let cg_image = CGImageCreate(
                width, height, 8, 32, width * 4, cs,
                bitmap_info, provider, std::ptr::null(), false, 0,
            );
            CGDataProviderRelease(provider);
            CGColorSpaceRelease(cs);
            if cg_image.is_null() {
                return Err("CGImageCreate 失败".to_string());
            }

            // 2. 准备输出 CFMutableData + CGImageDestination
            let out_data = CFDataCreateMutable(std::ptr::null_mut(), 0);
            if out_data.is_null() {
                CGImageRelease(cg_image);
                return Err("CFDataCreateMutable 失败".to_string());
            }
            // UTI string: "public.png" / "public.jpeg" → CFString
            let cstr = std::ffi::CString::new(uti).map_err(|e| e.to_string())?;
            // kCFStringEncodingUTF8 = 0x08000100
            let uti_cf = CFStringCreateWithCString(std::ptr::null_mut(), cstr.as_ptr(), 0x08000100);
            if uti_cf.is_null() {
                CFRelease(out_data);
                CGImageRelease(cg_image);
                return Err("CFString 创建失败".to_string());
            }
            let dst = CGImageDestinationCreateWithData(out_data, uti_cf, 1, std::ptr::null_mut());
            CFRelease(uti_cf);
            if dst.is_null() {
                CFRelease(out_data);
                CGImageRelease(cg_image);
                return Err("CGImageDestinationCreateWithData 失败".to_string());
            }

            // 3. 属性字典：JPEG 设 kCGImageDestinationLossyCompressionQuality
            let mut props_dict: *mut std::ffi::c_void = std::ptr::null_mut();
            if uti == "public.jpeg" {
                let key_cstr = std::ffi::CString::new("kCGImageDestinationLossyCompressionQuality").unwrap();
                let key = CFStringCreateWithCString(std::ptr::null_mut(), key_cstr.as_ptr(), 0x08000100);
                // kCFNumberDoubleType = 12
                let q = quality;
                let val = CFNumberCreate(std::ptr::null_mut(), 12, &q as *const f64 as *const std::ffi::c_void);
                if !key.is_null() && !val.is_null() {
                    let keys: [*const std::ffi::c_void; 1] = [key];
                    let vals: [*const std::ffi::c_void; 1] = [val];
                    props_dict = CFDictionaryCreate(
                        std::ptr::null_mut(),
                        keys.as_ptr(),
                        vals.as_ptr(),
                        1,
                        &kCFTypeDictionaryKeyCallBacks as *const std::ffi::c_void,
                        &kCFTypeDictionaryValueCallBacks as *const std::ffi::c_void,
                    );
                }
                if !key.is_null() { CFRelease(key); }
                if !val.is_null() { CFRelease(val); }
            }

            CGImageDestinationAddImage(dst, cg_image, props_dict);
            if !props_dict.is_null() { CFRelease(props_dict); }
            let ok = CGImageDestinationFinalize(dst);
            CFRelease(dst);
            CGImageRelease(cg_image);

            if !ok {
                CFRelease(out_data);
                return Err("CGImageDestinationFinalize 失败".to_string());
            }

            let len = CFDataGetLength(out_data) as usize;
            let bytes = CFDataGetBytePtr(out_data);
            let result = std::slice::from_raw_parts(bytes, len).to_vec();
            CFRelease(out_data);
            Ok(result)
        }
    }

    /// 计算每行的"签名"：行内所有像素值之和（u64，避免溢出）。
    /// 返回长度 = height 的 Vec。
    /// 一维签名比二维 SAD 快 O(width)，且对小噪声更鲁棒（GPU 合成抖动 / 子像素抗锯齿
    /// 在行求和后被平均掉）。
    fn row_signatures(frame: &[u8], width: usize, height: usize) -> Vec<u64> {
        let row_bytes = width * 4;
        let mut sigs = Vec::with_capacity(height);
        for r in 0..height {
            let s = r * row_bytes;
            let mut sum: u64 = 0;
            // 只算 RGB 通道（跳过 A）— alpha 在不透明窗口里恒为 255，没信息量
            let mut i = s;
            let end = s + row_bytes;
            while i < end {
                sum += frame[i] as u64 + frame[i + 1] as u64 + frame[i + 2] as u64;
                i += 4;
            }
            sigs.push(sum);
        }
        sigs
    }

    /// 在 new_frame 中找到 prev_frame 的最佳垂直对齐偏移。
    /// 返回 (k, direction, confidence)：
    ///   - confidence 是该偏移的可信度（avg 误差越小越可信）
    /// 算法：对前向和后向分别做行签名互相关，比较两个方向的最小归一化误差。
    pub fn find_scroll_offset(
        prev_frame: &[u8],
        new_frame: &[u8],
        width: usize,
        height: usize,
    ) -> (usize, ScrollDir, u64) {
        let prev_sigs = row_signatures(prev_frame, width, height);
        let new_sigs = row_signatures(new_frame, width, height);

        // 前向：prev[k+i] vs new[i]
        let mut fwd_best_k: usize = 0;
        let mut fwd_best_avg: u64 = u64::MAX;
        for k in 0..height {
            let n = height - k;
            if n < height / 4 {
                break;
            }
            let mut err: u64 = 0;
            for i in 0..n {
                let a = prev_sigs[k + i];
                let b = new_sigs[i];
                err += if a > b { a - b } else { b - a };
            }
            let avg = err / (n as u64);
            if avg < fwd_best_avg {
                fwd_best_avg = avg;
                fwd_best_k = k;
            }
        }

        // 后向：prev[i] vs new[k+i]
        let mut bwd_best_k: usize = 0;
        let mut bwd_best_avg: u64 = u64::MAX;
        for k in 1..height {
            let n = height - k;
            if n < height / 4 {
                break;
            }
            let mut err: u64 = 0;
            for i in 0..n {
                let a = prev_sigs[i];
                let b = new_sigs[k + i];
                err += if a > b { a - b } else { b - a };
            }
            let avg = err / (n as u64);
            if avg < bwd_best_avg {
                bwd_best_avg = avg;
                bwd_best_k = k;
            }
        }

        if bwd_best_avg < fwd_best_avg.saturating_mul(9) / 10 && bwd_best_k > 0 {
            (bwd_best_k, ScrollDir::Backward, bwd_best_avg)
        } else {
            (fwd_best_k, ScrollDir::Forward, fwd_best_avg)
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ScrollDir {
        Forward,
        Backward,
    }

    /// 把新帧追加到累积缓冲。返回（新增行数, 是否有变化）。
    ///
    /// 不变量：操作完成后 buf 底部 h 行 ≡ new_frame（字节级相等）。
    /// 这保证选区底部画面始终等于缓冲底部，反复来回滚动也不会累积漂移。
    ///
    /// 滚动过快防错乱：若最佳偏移的 confidence（avg 误差）相对该帧亮度均值
    /// 过大，说明帧间无可靠重叠（滚动幅度超过单帧），丢弃此帧——
    /// 不更新 prev_frame，等下一帧（用户减速时会有重叠）再尝试。
    pub fn append_frame(session: &mut ScrollSession, new_frame: Vec<u8>) -> (usize, bool) {
        let h = session.ph_per_frame;
        let rb = session.row_bytes();
        let frame_bytes = h * rb;

        if session.prev_frame.is_empty() {
            session.buf.extend_from_slice(&new_frame);
            session.total_rows += h;
            session.prev_frame = new_frame;
            return (h, true);
        }

        let (k, dir, confidence_err) = find_scroll_offset(
            &session.prev_frame, &new_frame, session.pw, h
        );

        // 可信度判断：err 阈值 = 帧亮度均值的 1/8。
        // 单行签名 = Σ(R+G+B)，一个像素行 RGB 全 255 时 = width * 765；
        // 平均行签名约 = width * 256（中等亮度）；
        // 阈值取 width * 32 即 1/8。
        // 重叠区无关时（完全不同的画面）avg 误差通常 ~width*100+，会被拒绝。
        let confidence_threshold: u64 = (session.pw as u64) * 32;
        if confidence_err > confidence_threshold {
            // 帧间无可靠重叠：丢弃此帧。不更新 prev_frame，让下一帧（可能减速后
            // 与原 prev 重叠）有机会匹配上。如果用户继续高速滚动，最多会丢几帧——
            // 比错乱拼接好得多。
            return (0, false);
        }

        let mut new_rows: usize = 0;
        let mut changed = false;

        match dir {
            ScrollDir::Backward => {
                let max_trim = session.total_rows.saturating_sub(h);
                if k <= max_trim {
                    let new_total = session.total_rows - k;
                    session.buf.truncate(new_total * rb);
                    session.total_rows = new_total;
                } else {
                    session.buf.clear();
                    session.buf.extend_from_slice(&new_frame);
                    session.total_rows = h;
                }
                changed = true;
            }
            ScrollDir::Forward => {
                if k > 0 {
                    let start = (h - k) * rb;
                    session.buf.extend_from_slice(&new_frame[start..]);
                    session.total_rows += k;
                    new_rows = k;
                    changed = true;
                }
            }
        }

        // 强制不变量：buf 底部 h 行 = new_frame（消除互相关精度误差累积漂移）
        if session.total_rows >= h {
            let buf_len = session.buf.len();
            let bottom_start = buf_len - frame_bytes;
            session.buf[bottom_start..buf_len].copy_from_slice(&new_frame);
        }

        session.prev_frame = new_frame;
        (new_rows, changed)
    }

    /// emit 节流：30fps（33ms）。返回是否应当 emit。
    pub fn should_emit() -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        let last = LAST_EMIT_NS.load(Ordering::Relaxed);
        if now.saturating_sub(last) < 33_000_000 {
            return false;
        }
        LAST_EMIT_NS.store(now, Ordering::Relaxed);
        true
    }

    /// 推送预览：把累积缓冲整体编码 JPEG → base64 → 前端。
    /// 不剪裁，前端用 max-width/max-height 等比 contain 完整显示，再长也不裁剪。
    pub fn emit_preview(app: &tauri::AppHandle, session: &mut ScrollSession) {
        if !should_emit() {
            return;
        }
        if session.total_rows == 0 || session.pw == 0 {
            return;
        }
        let jpeg = match encode_jpeg(&session.buf, session.pw, session.total_rows, 0.65) {
            Ok(d) => d,
            Err(_) => return,
        };
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg);
        session.emit_seq += 1;
        let _ = app.emit("screenshot-scroll-frame", serde_json::json!({
            "seq": session.emit_seq,
            "width": session.pw,
            "height": session.total_rows,
            "dataUrl": format!("data:image/jpeg;base64,{}", b64),
        }));
    }

    /// 后台抓帧线程：12ms 间隔；只做"抓帧 + stitch + emit + ignoring_mouse 状态广播"。
    /// 鼠标位置切 ignoresMouseEvents 已经移到 NSEvent monitor（主线程，事件驱动），
    /// 此循环不再做坐标轮询，避免后台线程 → 主线程同步的延迟和潜在死锁。
    pub fn capture_loop(app: tauri::AppHandle) {
        const FRAME_INTERVAL_MS: u64 = 12;
        let mut last_ignoring = false;

        while IS_RUNNING.load(Ordering::SeqCst) {
            let (sel_x, sel_y, sel_w, sel_h, overlay_id, cur_ignoring) = {
                let guard = SESSION.lock().unwrap();
                match guard.as_ref() {
                    Some(s) => (s.sel_x, s.sel_y, s.sel_w, s.sel_h, s.overlay_window_id, s.ignoring_mouse),
                    None => return,
                }
            };
            if cur_ignoring != last_ignoring {
                let _ = app.emit("screenshot-scroll-passthrough", cur_ignoring);
                last_ignoring = cur_ignoring;
            }

            // 抓帧 + stitch
            let frame = capture_below_overlay(sel_x, sel_y, sel_w, sel_h, overlay_id);
            if let Some((fw, fh, fbuf)) = frame {
                let mut guard = SESSION.lock().unwrap();
                if let Some(session) = guard.as_mut() {
                    if session.pw == 0 {
                        session.pw = fw;
                        session.ph_per_frame = fh;
                        eprintln!("[shot-scroll] first frame: {}x{}, buf_len={}, overlay_win_id={}", fw, fh, fbuf.len(), overlay_id);
                    }
                    if fw == session.pw && fh == session.ph_per_frame {
                        let (added, _changed) = append_frame(session, fbuf);
                        if session.emit_seq == 0 {
                            eprintln!("[shot-scroll] first append: added={} total_rows={}", added, session.total_rows);
                        }
                        emit_preview(&app, session);
                    }
                }
            } else if last_ignoring == cur_ignoring {
                // 只在第一次 None 时打印，避免刷屏
                static LOGGED_NONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
                if !LOGGED_NONE.swap(true, Ordering::Relaxed) {
                    eprintln!("[shot-scroll] capture_below_overlay returned None (sel={},{},{},{} win={})",
                        sel_x, sel_y, sel_w, sel_h, overlay_id);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(FRAME_INTERVAL_MS));
        }
    }

    /// NSEvent 鼠标监视器：在主线程（runloop）上每次鼠标移动都调一次，
    /// 同步切 ignoresMouseEvents。零延迟、无线程同步。
    /// 装一个 global monitor（应用未 active 时也收）+ 一个 local monitor（应用 active 时本地分发）。
    mod mouse_monitor {
        use objc2::runtime::AnyObject;
        use std::sync::Mutex;
        use super::SESSION;

        struct SendObj(*mut AnyObject);
        unsafe impl Send for SendObj {}
        unsafe impl Sync for SendObj {}

        static GLOBAL_MONITOR: Mutex<SendObj> = Mutex::new(SendObj(std::ptr::null_mut()));
        static LOCAL_MONITOR: Mutex<SendObj> = Mutex::new(SendObj(std::ptr::null_mut()));

        /// 鼠标坐标用 CGEvent 取（左上原点；C helper 包装结构体返回避开 Rust ABI）。
        unsafe fn cur_loc() -> (f64, f64) {
            let mut x: f64 = 0.0;
            let mut y: f64 = 0.0;
            super::super::voidnix_screenshot_get_mouse_location(&mut x, &mut y, 0.0);
            (x, y)
        }

        unsafe fn check_and_toggle() {
            let (mx, my) = cur_loc();
            let snapshot = {
                let g = SESSION.lock().unwrap();
                match g.as_ref() {
                    Some(s) => Some((s.sel_x, s.sel_y, s.sel_w, s.sel_h, s.ns_window_addr, s.ignoring_mouse)),
                    None => None,
                }
            };
            let Some((sx, sy, sw, sh, ns_addr, currently_ignoring)) = snapshot else { return };
            let in_hole = mx >= sx && mx <= sx + sw && my >= sy && my <= sy + sh;
            if in_hole != currently_ignoring {
                let ptr = ns_addr as *mut std::ffi::c_void;
                super::super::voidnix_screenshot_set_ignores_mouse(ptr, if in_hole { 1 } else { 0 });
                let mut g = SESSION.lock().unwrap();
                if let Some(s) = g.as_mut() {
                    s.ignoring_mouse = in_hole;
                }
                // emit 给前端做可视反馈
                // 注意：tauri 的 emit 需要 AppHandle；这里通过线程局部 hack 较丑，
                // 改在 capture_loop 里用 ignoring_mouse 状态变化时 emit。
            }
        }

        pub fn start() {
            use objc2::ClassType;
            use objc2_app_kit::NSEvent;
            { let g = GLOBAL_MONITOR.lock().unwrap(); if !g.0.is_null() { return; } }
            // mask: MouseMoved (1<<5) | LeftMouseDragged (1<<6) | RightMouseDragged (1<<7)
            //     | OtherMouseDragged (1<<27) | ScrollWheel (1<<22)
            //     | LeftMouseDown (1<<1) | LeftMouseUp (1<<2) | RightMouseDown (1<<3) | RightMouseUp (1<<4)
            // 包含 ScrollWheel 是关键：用户可能不移动鼠标直接滚轮，此时 MouseMoved 不触发。
            let mask: u64 = (1u64 << 5) | (1u64 << 6) | (1u64 << 7) | (1u64 << 27)
                | (1u64 << 22) | (1u64 << 1) | (1u64 << 2) | (1u64 << 3) | (1u64 << 4);
            // global: 应用未 active 时（用户在底下应用上移动鼠标）
            {
                let blk = block2::RcBlock::new(move |_event: *mut AnyObject| {
                    unsafe { check_and_toggle(); }
                });
                unsafe {
                    let m: *mut AnyObject = objc2::msg_send![NSEvent::class(), addGlobalMonitorForEventsMatchingMask: mask, handler: &*blk];
                    if !m.is_null() {
                        let _: () = objc2::msg_send![m, retain];
                        *GLOBAL_MONITOR.lock().unwrap() = SendObj(m);
                        std::mem::forget(blk);
                    }
                }
            }
            // local: 截屏窗口接收事件时（ignoresMouseEvents=NO 时）
            {
                let blk = block2::RcBlock::new(move |event: *mut AnyObject| -> *mut AnyObject {
                    unsafe { check_and_toggle(); }
                    event
                });
                unsafe {
                    let m: *mut AnyObject = objc2::msg_send![NSEvent::class(), addLocalMonitorForEventsMatchingMask: mask, handler: &*blk];
                    if !m.is_null() {
                        let _: () = objc2::msg_send![m, retain];
                        *LOCAL_MONITOR.lock().unwrap() = SendObj(m);
                        std::mem::forget(blk);
                    }
                }
            }
        }

        pub fn stop() {
            use objc2::ClassType;
            use objc2_app_kit::NSEvent;
            for slot in [&GLOBAL_MONITOR, &LOCAL_MONITOR] {
                let mut g = slot.lock().unwrap();
                if !g.0.is_null() {
                    unsafe {
                        let _: () = objc2::msg_send![NSEvent::class(), removeMonitor: g.0];
                        let _: () = objc2::msg_send![g.0, release];
                    }
                    *g = SendObj(std::ptr::null_mut());
                }
            }
        }
    }

    pub use mouse_monitor::{start as start_mouse_monitor, stop as stop_mouse_monitor};
}

/// 进入滚动截屏模式。前端在按下"滚动截屏"按钮后调；
/// 后端装上挖洞遮罩 → 启动抓帧线程；后续每帧通过 emit("screenshot-scroll-frame") 推预览。
#[tauri::command]
pub async fn enter_scroll_capture(
    app: tauri::AppHandle,
    sel_x: f64, sel_y: f64, sel_w: f64, sel_h: f64,
    scale: f64,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use std::sync::atomic::Ordering;
        // 防止重入
        if scroll_capture::IS_RUNNING.load(Ordering::SeqCst) {
            return Err("滚动截屏已在进行中".to_string());
        }

        // 主线程：装挖洞遮罩 + 取 windowNumber + ns_window 地址
        let (tx, rx) = std::sync::mpsc::channel::<Result<(u32, usize), String>>();
        let app_c = app.clone();
        app.run_on_main_thread(move || {
            use tauri::Manager;
            use objc2_app_kit::NSWindow;
            let r = (|| -> Result<(u32, usize), String> {
                let window = app_c.get_webview_window("screenshot").ok_or("找不到截图窗口")?;
                let raw = window.ns_window().map_err(|e| e.to_string())?.cast::<NSWindow>();
                let ptr = raw.cast::<NSWindow>() as *mut std::ffi::c_void;
                let ns_addr = ptr as usize;
                unsafe {
                    if !voidnix_screenshot_install_scroll_mask(ptr, sel_x, sel_y, sel_w, sel_h) {
                        return Err("装载滚动遮罩失败".to_string());
                    }
                    // 关键：把窗口设为 NSWindowSharingNone，让 CGWindowListCreateImage
                    // 永远不会把我们的窗口合成进截图（这是 Shottr/CleanShot 等工具的标准做法）
                    voidnix_screenshot_set_sharing(ptr, 0);
                    let win_num = voidnix_screenshot_window_number(ptr);
                    if win_num <= 0 {
                        return Err("获取截屏窗口编号失败".to_string());
                    }
                    Ok((win_num as u32, ns_addr))
                }
            })();
            let _ = tx.send(r);
        }).map_err(|e| e.to_string())?;
        let (overlay_window_id, ns_window_addr) = rx.recv().map_err(|e| e.to_string())??;

        // 初始化会话
        {
            let mut guard = scroll_capture::SESSION.lock().unwrap();
            *guard = Some(scroll_capture::ScrollSession {
                sel_x, sel_y, sel_w, sel_h, scale,
                pw: 0,
                ph_per_frame: 0,
                buf: Vec::new(),
                total_rows: 0,
                prev_frame: Vec::new(),
                overlay_window_id,
                ns_window_addr,
                ignoring_mouse: false,
                emit_seq: 0,
            });
        }

        // SESSION 已就绪，再启动鼠标监视器（监视器立刻可能 fire）
        let (tx2, rx2) = std::sync::mpsc::channel::<()>();
        app.run_on_main_thread(move || {
            scroll_capture::start_mouse_monitor();
            // 立即做一次 toggle 检查：用户可能已经把鼠标放在选区内再点的按钮
            unsafe {
                let mut mx: f64 = 0.0;
                let mut my: f64 = 0.0;
                voidnix_screenshot_get_mouse_location(&mut mx, &mut my, 0.0);
                let g = scroll_capture::SESSION.lock().unwrap();
                if let Some(s) = g.as_ref() {
                    let in_hole = mx >= s.sel_x && mx <= s.sel_x + s.sel_w
                        && my >= s.sel_y && my <= s.sel_y + s.sel_h;
                    if in_hole {
                        let ptr = s.ns_window_addr as *mut std::ffi::c_void;
                        voidnix_screenshot_set_ignores_mouse(ptr, 1);
                        drop(g);
                        let mut g2 = scroll_capture::SESSION.lock().unwrap();
                        if let Some(s2) = g2.as_mut() {
                            s2.ignoring_mouse = true;
                        }
                    }
                }
            }
            let _ = tx2.send(());
        }).map_err(|e| e.to_string())?;
        let _ = rx2.recv();

        scroll_capture::IS_RUNNING.store(true, Ordering::SeqCst);

        // 启动抓帧线程
        std::thread::spawn(move || scroll_capture::capture_loop(app));
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, sel_x, sel_y, sel_w, sel_h, scale);
        Err("仅支持 macOS".to_string())
    }
}

/// 退出滚动截屏（取消，不保留累积缓冲）。
#[tauri::command]
pub async fn exit_scroll_capture(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use std::sync::atomic::Ordering;
        scroll_capture::IS_RUNNING.store(false, Ordering::SeqCst);
        // 等线程退出（最多 50ms）
        std::thread::sleep(std::time::Duration::from_millis(50));
        {
            let mut guard = scroll_capture::SESSION.lock().unwrap();
            *guard = None;
        }

        let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let app_c = app.clone();
        app.run_on_main_thread(move || {
            use tauri::Manager;
            use objc2_app_kit::NSWindow;
            let r = (|| -> Result<(), String> {
                // 先停 NSEvent 监视器（必须主线程）
                scroll_capture::stop_mouse_monitor();
                let window = app_c.get_webview_window("screenshot").ok_or("找不到截图窗口")?;
                let raw = window.ns_window().map_err(|e| e.to_string())?.cast::<NSWindow>();
                let ptr = raw.cast::<NSWindow>() as *mut std::ffi::c_void;
                unsafe {
                    voidnix_screenshot_remove_scroll_mask(ptr);
                    voidnix_screenshot_set_sharing(ptr, 1); // 恢复 ReadOnly
                }
                Ok(())
            })();
            let _ = tx.send(r);
        }).map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())??;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Ok(())
    }
}

/// 完成滚动截屏：停止抓帧、移除遮罩，把累积缓冲编码 PNG 返回（base64 dataURL）。
#[tauri::command]
pub async fn finish_scroll_capture(app: tauri::AppHandle) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        use std::sync::atomic::Ordering;
        scroll_capture::IS_RUNNING.store(false, Ordering::SeqCst);
        std::thread::sleep(std::time::Duration::from_millis(50));

        // 取出会话并 drop guard，避免编码期间锁住
        let session = {
            let mut guard = scroll_capture::SESSION.lock().unwrap();
            guard.take()
        };
        let session = session.ok_or("无滚动截屏会话".to_string())?;
        if session.total_rows == 0 || session.pw == 0 {
            return Err("未捕获到任何内容".to_string());
        }

        // 主线程：停监视器 + 移除遮罩 + 恢复 sharing
        let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let app_c = app.clone();
        app.run_on_main_thread(move || {
            use tauri::Manager;
            use objc2_app_kit::NSWindow;
            let r = (|| -> Result<(), String> {
                scroll_capture::stop_mouse_monitor();
                let window = app_c.get_webview_window("screenshot").ok_or("找不到截图窗口")?;
                let raw = window.ns_window().map_err(|e| e.to_string())?.cast::<NSWindow>();
                let ptr = raw.cast::<NSWindow>() as *mut std::ffi::c_void;
                unsafe {
                    voidnix_screenshot_remove_scroll_mask(ptr);
                    voidnix_screenshot_set_sharing(ptr, 1);
                }
                Ok(())
            })();
            let _ = tx.send(r);
        }).map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())??;

        // 编码 PNG
        let png = scroll_capture::encode_png(&session.buf, session.pw, session.total_rows)?;
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
        Ok(format!("data:image/png;base64,{}", b64))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err("仅支持 macOS".to_string())
    }
}

/// 把滚动截屏结果保存到指定路径（png）。result_data_url 由 finish_scroll_capture 返回。
#[tauri::command]
pub async fn save_scroll_result(
    result_data_url: String,
    path: String,
) -> Result<String, String> {
    let png = decode_image_data(&result_data_url)?;
    let file_path = {
        let p = std::path::Path::new(&path);
        if p.is_dir() {
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
            p.join(format!("scroll_screenshot_{}.png", ts))
        } else {
            p.to_path_buf()
        }
    };
    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&file_path, png).map_err(|e| e.to_string())?;
    Ok(file_path.to_string_lossy().to_string())
}

/// 复制滚动截屏结果到剪贴板。
#[tauri::command]
pub async fn copy_scroll_result_to_clipboard(result_data_url: String) -> Result<(), String> {
    let png = decode_image_data(&result_data_url)?;
    #[cfg(target_os = "macos")]
    {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis();
        let tmp = std::env::temp_dir().join(format!("voidnix_scroll_{}.png", ts));
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

pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new("screenshot")
        .setup(|_app, _api| {
            #[cfg(target_os = "macos")]
            {
                use tauri::Emitter;
                crate::core::shortcut::register_shortcut_hook("screenshot", Box::new(|app, _ctx| {
                    // 已在截屏会话中：屏蔽重复触发，避免叠加 capture 与 overlay。
                    // swap 拿独占权，true→true 表示已占；false→true 表示本次抢到。
                    if IS_IN_SCREENSHOT_SESSION.swap(true, std::sync::atomic::Ordering::SeqCst) {
                        return true;
                    }
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
                                // capture 失败需立刻释放锁，否则后续快捷键全被屏蔽
                                IS_IN_SCREENSHOT_SESSION.store(false, std::sync::atomic::Ordering::SeqCst);
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
    // 清理 awake 临时可执行文件
    let awake_dir = temp_dir.join("com.litiantao.voidnix");
    let awake_bin = awake_dir.join("Display Wakelock");
    let _ = std::fs::remove_file(&awake_bin);
    let _ = std::fs::remove_dir(&awake_dir);
}
