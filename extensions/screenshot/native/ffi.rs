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
    pub data_url: String,
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
    let mut guard = CURRENT_CG_IMAGE.lock().unwrap_or_else(|e| e.into_inner());
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
    CURRENT_CG_IMAGE.lock().unwrap_or_else(|e| e.into_inner()).0
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

#[cfg(target_os = "macos")]
pub(super) fn prepare_picker_jpeg(raw: *mut std::ffi::c_void) {
    if raw.is_null() {
        eprintln!("[shot] prepare_picker_jpeg: raw is null");
        return;
    }
    match cg_image_ptr_to_jpeg(raw) {
        Ok(data) => {
            let path = picker_jpeg_path();
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(e) = std::fs::write(path, &data) {
                eprintln!("[shot] prepare_picker_jpeg write error: {e}");
            }
        }
        Err(e) => eprintln!("[shot] prepare_picker_jpeg encode error: {e}"),
    }
}

#[cfg(target_os = "macos")]
pub(super) fn cg_image_ptr_to_jpeg(raw: *mut std::ffi::c_void) -> Result<Vec<u8>, String> {
    use objc2::runtime::AnyObject;
    if raw.is_null() {
        return Err("CGImage 为空".to_string());
    }
    // SAFETY: raw 已 null 检查；alloc/initWithCGImage:/representationUsingType:properties:/
    // release 均为 NSBitmapImageRep/NSNumber/NSDictionary 标准选择子，参数类型匹配；
    // ns_data 释放前先 length/bytes 拷贝出数据（NSData 字节缓冲区在 release 前有效）
    unsafe {
        let cls = objc2::class!(NSBitmapImageRep);
        let rep: *mut AnyObject = objc2::msg_send![cls, alloc];
        let rep: *mut AnyObject = objc2::msg_send![rep, initWithCGImage: raw];
        if rep.is_null() {
            return Err("NSBitmapImageRep initWithCGImage 失败".to_string());
        }
        let key = objc2_foundation::NSString::from_str("NSImageCompressionFactor");
        let val: *mut AnyObject = objc2::msg_send![
            objc2::class!(NSNumber),
            numberWithDouble: 0.95f64
        ];
        let props: *mut AnyObject = objc2::msg_send![
            objc2::class!(NSDictionary),
            dictionaryWithObject: val,
            forKey: &*key
        ];
        let ns_data: *mut AnyObject =
            objc2::msg_send![rep, representationUsingType: 3usize, properties: props];
        let _: () = objc2::msg_send![rep, release];
        if ns_data.is_null() {
            return Err("JPEG 编码失败".to_string());
        }
        let length: usize = objc2::msg_send![ns_data, length];
        let bytes: *const u8 = objc2::msg_send![ns_data, bytes];
        Ok(std::slice::from_raw_parts(bytes, length).to_vec())
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

#[cfg(target_os = "macos")]
pub fn prewarm_jpeg_encoder() {
    use objc2::runtime::AnyObject;
    // SAFETY: 仅为预热编码器：alloc + initWithBitmapDataPlanes:(1×1 空位图) +
    // representationUsingType: 一次后 release；参数类型匹配，rep null 检查后操作
    unsafe {
        let cls = objc2::class!(NSBitmapImageRep);
        let rep: *mut AnyObject = objc2::msg_send![cls, alloc];
        let cs = objc2_foundation::NSString::from_str("NSDeviceRGBColorSpace");
        let null_planes: *mut *mut u8 = std::ptr::null_mut();
        let rep: *mut AnyObject = objc2::msg_send![
            rep,
            initWithBitmapDataPlanes: null_planes,
            pixelsWide: 1isize,
            pixelsHigh: 1isize,
            bitsPerSample: 8isize,
            samplesPerPixel: 4isize,
            hasAlpha: true,
            isPlanar: false,
            colorSpaceName: &*cs,
            bytesPerRow: 0isize,
            bitsPerPixel: 0isize
        ];
        if rep.is_null() {
            return;
        }
        let key = objc2_foundation::NSString::from_str("NSImageCompressionFactor");
        let val: *mut AnyObject = objc2::msg_send![
            objc2::class!(NSNumber),
            numberWithDouble: 0.95f64
        ];
        let props: *mut AnyObject = objc2::msg_send![
            objc2::class!(NSDictionary),
            dictionaryWithObject: val,
            forKey: &*key
        ];
        let _: *mut AnyObject =
            objc2::msg_send![rep, representationUsingType: 3usize, properties: props];
        let _: () = objc2::msg_send![rep, release];
    }
}

#[cfg(not(target_os = "macos"))]
pub fn prewarm_jpeg_encoder() {}
