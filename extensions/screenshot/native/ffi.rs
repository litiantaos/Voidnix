use crate::runtime::lock_or_recover;
use base64::Engine;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WindowRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub owner: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TextRegion {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OcrResult {
    pub text: String,
    pub qr: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ScreenshotData {
    pub width: u32,
    pub height: u32,
    pub scale: f64,
    pub mouse_x: f64,
    pub mouse_y: f64,
    pub windows: Vec<WindowRect>,
}

#[cfg(target_os = "macos")]
extern "C" {
    pub(crate) fn voidnix_screenshot_install_background_layer(
        ns_window_ptr: *mut std::ffi::c_void,
    ) -> bool;
    pub(crate) fn voidnix_screenshot_set_background(
        ns_window_ptr: *mut std::ffi::c_void,
        cg_image_ptr: *mut std::ffi::c_void,
    ) -> bool;
    pub(crate) fn voidnix_screenshot_set_background_centered(
        ns_window_ptr: *mut std::ffi::c_void,
        cg_image_ptr: *mut std::ffi::c_void,
    ) -> bool;
    pub(crate) fn voidnix_screenshot_clear_background(ns_window_ptr: *mut std::ffi::c_void);
    pub(crate) fn voidnix_screenshot_install_scroll_mask(
        ns_window_ptr: *mut std::ffi::c_void,
        sel_x: f64,
        sel_y: f64,
        sel_w: f64,
        sel_h: f64,
    ) -> bool;
    pub(crate) fn voidnix_screenshot_remove_scroll_mask(ns_window_ptr: *mut std::ffi::c_void);
    pub(crate) fn voidnix_screenshot_window_number(
        ns_window_ptr: *mut std::ffi::c_void,
    ) -> std::os::raw::c_long;
    pub(crate) fn voidnix_screenshot_set_ignores_mouse(
        ns_window_ptr: *mut std::ffi::c_void,
        ignores: i32,
    );
    pub(crate) fn voidnix_screenshot_get_mouse_location(
        out_x: *mut f64,
        out_y: *mut f64,
        screen_height: f64,
    );
    pub(crate) fn voidnix_screenshot_capture_region(
        ns_window_ptr: *mut std::ffi::c_void,
        sel_x: f64,
        sel_y: f64,
        sel_w: f64,
        sel_h: f64,
        out_image: *mut *mut std::ffi::c_void,
    );
    pub(crate) fn voidnix_screenshot_set_sharing(
        ns_window_ptr: *mut std::ffi::c_void,
        sharing: i32,
    );
    /// activate + makeKey + firstResponder=WKWebView（换屏后重申 key）
    pub(crate) fn voidnix_screenshot_claim_key(ns_window_ptr: *mut std::ffi::c_void);
    /// 冷启动预热 WKWebView（不 activate）
    pub(crate) fn voidnix_screenshot_prewarm(ns_window_ptr: *mut std::ffi::c_void);
}

#[cfg(target_os = "macos")]
extern "C" {
    pub(crate) fn CGImageRetain(image: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    pub(crate) fn CGImageRelease(image: *mut std::ffi::c_void);
}

#[cfg(target_os = "macos")]
struct SendCgImage(*mut std::ffi::c_void);
#[cfg(target_os = "macos")]
unsafe impl Send for SendCgImage {}
#[cfg(target_os = "macos")]
unsafe impl Sync for SendCgImage {}

#[cfg(target_os = "macos")]
static CURRENT_CG_IMAGE: std::sync::Mutex<SendCgImage> =
    std::sync::Mutex::new(SendCgImage(std::ptr::null_mut()));

#[cfg(target_os = "macos")]
pub(super) fn store_cg_image(raw: *mut std::ffi::c_void) {
    let mut guard = lock_or_recover(&CURRENT_CG_IMAGE);
    let old = guard.0;
    if !old.is_null() {
        // SAFETY: old 已非空校验；CGImageRelease 遵循 CG ownership（替换前释放旧引用）
        unsafe { CGImageRelease(old) };
    }
    if !raw.is_null() {
        // SAFETY: raw 已非空校验；CGImageRetain +1 引用计数，由 CURRENT_CG_IMAGE 持有
        unsafe { CGImageRetain(raw) };
    }
    guard.0 = raw;
}

#[cfg(target_os = "macos")]
pub(super) fn get_cg_image() -> *mut std::ffi::c_void {
    lock_or_recover(&CURRENT_CG_IMAGE).0
}

pub(super) fn decode_image_data(s: &str) -> Result<Vec<u8>, String> {
    let b64 = if let Some(p) = s.find(',') {
        &s[p + 1..]
    } else {
        s
    };
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| e.to_string())
}

pub fn picker_jpeg_path() -> std::path::PathBuf {
    std::env::temp_dir().join("voidnix").join("picker.jpg")
}

/// 递增取消在途的旧编码任务（capture 成功 / 失败清理时调用）。
static PICKER_JOB: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// 取消在途 picker 编码任务并清掉残文件。
#[cfg(target_os = "macos")]
pub(super) fn cancel_picker_jobs() {
    use std::sync::atomic::Ordering;
    PICKER_JOB.fetch_add(1, Ordering::SeqCst);
    let path = picker_jpeg_path();
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("jpg.tmp"));
}

#[cfg(not(target_os = "macos"))]
pub(super) fn cancel_picker_jobs() {}

/// capture 完成后立即异步编码 picker.jpg，与 enter 并行。
/// 主屏 Retina 编码可达数百 ms，必须早于 Vue mount 启动，前端仍需轮询就绪。
/// 任务持有 CGImage 独立 Retain，不依赖 CURRENT_CG_IMAGE 存活。
#[cfg(target_os = "macos")]
pub(super) fn start_prepare_picker_jpeg() {
    use std::sync::atomic::Ordering;
    let job = PICKER_JOB.fetch_add(1, Ordering::SeqCst) + 1;
    let raw = get_cg_image();
    if raw.is_null() {
        return;
    }
    // 为后台任务 +1；job 结束时 Release，与 store_cg_image 生命周期解耦
    // SAFETY: raw 非空，CURRENT_CG_IMAGE 持有的有效 CGImageRef
    unsafe { CGImageRetain(raw) };
    // 先清旧文件，避免前端读到上一会话残图
    let _ = std::fs::remove_file(picker_jpeg_path());
    let raw_addr = raw as usize;
    std::thread::spawn(move || {
        let ptr = raw_addr as *mut std::ffi::c_void;
        prepare_picker_jpeg_job(ptr, job);
        // SAFETY: 与上方 CGImageRetain 配对
        unsafe { CGImageRelease(ptr) };
    });
}

#[cfg(not(target_os = "macos"))]
pub(super) fn start_prepare_picker_jpeg() {}

#[cfg(target_os = "macos")]
fn prepare_picker_jpeg_job(raw: *mut std::ffi::c_void, job: u64) {
    use std::sync::atomic::Ordering;
    if raw.is_null() {
        eprintln!("[shot] prepare_picker_jpeg: raw is null");
        return;
    }
    match cg_image_ptr_to_jpeg(raw) {
        Ok(data) => {
            // 已被更新会话取代则丢弃
            if PICKER_JOB.load(Ordering::SeqCst) != job {
                return;
            }
            let path = picker_jpeg_path();
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            // 原子落盘：写 tmp 再 rename，避免前端读到半截 JPEG
            let tmp = path.with_extension("jpg.tmp");
            if let Err(e) = std::fs::write(&tmp, &data) {
                eprintln!("[shot] prepare_picker_jpeg write tmp error: {e}");
                return;
            }
            if PICKER_JOB.load(Ordering::SeqCst) != job {
                let _ = std::fs::remove_file(&tmp);
                return;
            }
            if let Err(e) = std::fs::rename(&tmp, &path) {
                eprintln!("[shot] prepare_picker_jpeg rename error: {e}");
                let _ = std::fs::remove_file(&tmp);
            }
        }
        Err(e) => eprintln!("[shot] prepare_picker_jpeg encode error: {e}"),
    }
}

/// ImageIO 直编 JPEG（线程安全，不经 AppKit NSBitmapImageRep）。
#[cfg(target_os = "macos")]
pub(super) fn cg_image_ptr_to_jpeg(raw: *mut std::ffi::c_void) -> Result<Vec<u8>, String> {
    if raw.is_null() {
        return Err("CGImage 为空".to_string());
    }
    // SAFETY: raw 非空 CGImageRef；CGImageDestination* / CF* 为 ImageIO/CoreFoundation
    // 线程安全 API。Create 返回值均 null 检查，CFRelease 配平；from_raw_parts 在
    // out_data release 前拷贝。
    unsafe {
        extern "C" {
            fn CFDataCreateMutable(
                allocator: *mut std::ffi::c_void,
                capacity: isize,
            ) -> *mut std::ffi::c_void;
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

        let out_data = CFDataCreateMutable(std::ptr::null_mut(), 0);
        if out_data.is_null() {
            return Err("CFDataCreateMutable 失败".to_string());
        }
        let cstr = std::ffi::CString::new("public.jpeg").map_err(|e| e.to_string())?;
        // kCFStringEncodingUTF8 = 0x08000100
        let uti_cf = CFStringCreateWithCString(std::ptr::null_mut(), cstr.as_ptr(), 0x08000100);
        if uti_cf.is_null() {
            CFRelease(out_data);
            return Err("CFString 创建失败".to_string());
        }
        let dst = CGImageDestinationCreateWithData(out_data, uti_cf, 1, std::ptr::null_mut());
        CFRelease(uti_cf);
        if dst.is_null() {
            CFRelease(out_data);
            return Err("CGImageDestinationCreateWithData 失败".to_string());
        }

        let key_cstr =
            std::ffi::CString::new("kCGImageDestinationLossyCompressionQuality").unwrap();
        let key = CFStringCreateWithCString(std::ptr::null_mut(), key_cstr.as_ptr(), 0x08000100);
        let q: f64 = 0.95;
        // kCFNumberDoubleType = 13
        let val = CFNumberCreate(
            std::ptr::null_mut(),
            13,
            &q as *const f64 as *const std::ffi::c_void,
        );
        let mut props_dict: *mut std::ffi::c_void = std::ptr::null_mut();
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
        if !key.is_null() {
            CFRelease(key);
        }
        if !val.is_null() {
            CFRelease(val);
        }

        CGImageDestinationAddImage(dst, raw, props_dict);
        if !props_dict.is_null() {
            CFRelease(props_dict);
        }
        let ok = CGImageDestinationFinalize(dst);
        CFRelease(dst);
        if !ok {
            CFRelease(out_data);
            return Err("CGImageDestinationFinalize 失败".to_string());
        }

        let len = CFDataGetLength(out_data) as usize;
        let bytes = CFDataGetBytePtr(out_data);
        if bytes.is_null() || len == 0 {
            CFRelease(out_data);
            return Err("JPEG 编码结果为空".to_string());
        }
        let result = std::slice::from_raw_parts(bytes, len).to_vec();
        CFRelease(out_data);
        Ok(result)
    }
}

#[cfg(target_os = "macos")]
pub(super) fn png_bytes_to_cgimage(png: &[u8]) -> *mut std::ffi::c_void {
    use objc2::runtime::AnyObject;
    if png.is_empty() {
        return std::ptr::null_mut();
    }
    // SAFETY: png 非空（已检查）；dataWithBytes:length:/imageRepWithData:/CGImage 均
    // 为 NSData/NSBitmapImageRep 标准选择子；返回的 cg 经 CGImageRetain +1（调用方释放）
    unsafe {
        let cls_data = objc2::class!(NSData);
        let ns_data: *mut AnyObject = objc2::msg_send![
            cls_data,
            dataWithBytes: png.as_ptr() as *const std::ffi::c_void,
            length: png.len()
        ];
        if ns_data.is_null() {
            return std::ptr::null_mut();
        }
        let cls_rep = objc2::class!(NSBitmapImageRep);
        let rep: *mut AnyObject = objc2::msg_send![cls_rep, imageRepWithData: ns_data];
        if rep.is_null() {
            return std::ptr::null_mut();
        }
        let cg: *mut std::ffi::c_void = objc2::msg_send![rep, CGImage];
        if cg.is_null() {
            return std::ptr::null_mut();
        }
        CGImageRetain(cg)
    }
}

/// 预热 ImageIO JPEG 路径（与后台编码同源，线程安全）。
#[cfg(target_os = "macos")]
pub fn prewarm_jpeg_encoder() {
    // SAFETY: 1×1 BGRA → CGImageCreate → ImageIO 走一遍，Create/Release 配平
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
        }
        let px: [u8; 4] = [0, 0, 0, 255];
        let cs = CGColorSpaceCreateDeviceRGB();
        let provider = CGDataProviderCreateWithData(
            std::ptr::null_mut(),
            px.as_ptr(),
            4,
            std::ptr::null_mut(),
        );
        if provider.is_null() {
            CGColorSpaceRelease(cs);
            return;
        }
        // kCGImageAlphaPremultipliedLast | kCGBitmapByteOrder32Big
        let bitmap_info: u32 = 2 | (2 << 12);
        let cg = CGImageCreate(
            1,
            1,
            8,
            32,
            4,
            cs,
            bitmap_info,
            provider,
            std::ptr::null(),
            false,
            0,
        );
        CGDataProviderRelease(provider);
        CGColorSpaceRelease(cs);
        if cg.is_null() {
            return;
        }
        let _ = cg_image_ptr_to_jpeg(cg);
        CGImageRelease(cg);
    }
}

#[cfg(not(target_os = "macos"))]
pub fn prewarm_jpeg_encoder() {}
