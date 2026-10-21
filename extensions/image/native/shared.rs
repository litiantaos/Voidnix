//! remove_bg 与 stitch 共用的图片加载、PNG 编码、结果构建、临时文件管理。

use base64::Engine;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send, AnyThread};
use objc2_app_kit::{NSBitmapImageFileType, NSImage};
use objc2_foundation::{NSDictionary, NSString};
use serde::Serialize;
use std::ffi::c_void;
use std::path::Path;

use crate::platform::path_guard;

/// 处理结果（remove_bg 与 stitch 共用，IPC 边界类型）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageResult {
    /// data:image/png;base64,... 供前端直接 <img :src>
    pub preview_data_url: String,
    /// 临时文件路径（供 save_result / copy_to_clipboard 复用，不重复处理）
    pub temp_path: String,
    pub width: u32,
    pub height: u32,
    pub size_bytes: u64,
}

/// 管道提取出的原始 PNG 数据（已脱离 ObjC 所有权）。
pub struct ExtractedImage {
    pub png_bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// 已加载的图片。`ns_image` 持有所有权，保持 `cg_image` 生命周期有效。
pub struct Loaded {
    _ns_image: Retained<NSImage>,
    pub cg_image: *mut c_void,
    pub width: u32,
    pub height: u32,
}

// CoreGraphics 维度查询（符号经 AppKit 传递链接）
extern "C" {
    fn CGImageGetWidth(image: *mut c_void) -> usize;
    fn CGImageGetHeight(image: *mut c_void) -> usize;
    fn CFRelease(cf: *mut c_void);
}

// ImageIO JPEG 编码（云端接口输入压缩；线程安全，不经 AppKit）
#[link(name = "ImageIO", kind = "framework")]
extern "C" {
    fn CFDataCreateMutable(allocator: *mut c_void, capacity: isize) -> *mut c_void;
    fn CFStringCreateWithCString(
        allocator: *mut c_void,
        cstr: *const std::os::raw::c_char,
        encoding: u32,
    ) -> *mut c_void;
    fn CGImageDestinationCreateWithData(
        data: *mut c_void,
        uti: *const c_void,
        count: usize,
        options: *mut c_void,
    ) -> *mut c_void;
    fn CGImageDestinationAddImage(dst: *mut c_void, image: *mut c_void, properties: *mut c_void);
    fn CGImageDestinationFinalize(dst: *mut c_void) -> bool;
    fn CFDataGetLength(data: *mut c_void) -> isize;
    fn CFDataGetBytePtr(data: *mut c_void) -> *const u8;
    fn CFNumberCreate(
        allocator: *mut c_void,
        number_type: i32,
        value: *const c_void,
    ) -> *mut c_void;
    fn CFDictionaryCreate(
        allocator: *mut c_void,
        keys: *const *const c_void,
        values: *const *const c_void,
        num_values: isize,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> *mut c_void;
    static kCFTypeDictionaryKeyCallBacks: c_void;
    static kCFTypeDictionaryValueCallBacks: c_void;
}

/// 加载图片文件为 CGImage（NSImage 支持所有 macOS 原生格式）。
///
/// 返回的 `Loaded` 持有 NSImage 所有权——调用方须保持其存活直到不再需要 CGImage。
/// path_guard 安全校验。
pub unsafe fn load_image(path: &str) -> Result<Loaded, String> {
    if !path_guard::validate(Path::new(path)) {
        return Err(format!("路径不安全或不存在：{path}"));
    }

    let ns_path = NSString::from_str(path);
    let ns_image: Option<Retained<NSImage>> =
        msg_send![NSImage::alloc(), initWithContentsOfFile: &*ns_path];
    let ns_image = ns_image.ok_or(format!("无法加载图片（格式不支持或文件损坏）：{path}"))?;

    let cg_image: *mut c_void = msg_send![
        &*ns_image,
        CGImageForProposedRect: std::ptr::null_mut::<objc2_foundation::NSRect>(),
        context: std::ptr::null::<AnyObject>(),
        hints: std::ptr::null::<AnyObject>()
    ];
    if cg_image.is_null() {
        return Err(format!("无法从图片提取 CGImage：{path}"));
    }

    let width = CGImageGetWidth(cg_image) as u32;
    let height = CGImageGetHeight(cg_image) as u32;

    Ok(Loaded {
        _ns_image: ns_image,
        cg_image,
        width,
        height,
    })
}

/// CGImage → PNG 字节（经 NSBitmapImageRep 编码）。
///
/// 调用方须在 autoreleasepool 内调用（representationUsingType 返回自动释放对象）。
pub unsafe fn encode_png(cg_image: *mut c_void) -> Result<Vec<u8>, String> {
    let bitmap_rep: *mut AnyObject = msg_send![class!(NSBitmapImageRep), alloc];
    let bitmap_rep: *mut AnyObject = msg_send![bitmap_rep, initWithCGImage: cg_image];
    if bitmap_rep.is_null() {
        return Err("NSBitmapImageRep 创建失败".into());
    }

    let empty = NSDictionary::<NSString, AnyObject>::new();
    let png_data: *mut objc2_foundation::NSData = msg_send![
        bitmap_rep,
        representationUsingType: NSBitmapImageFileType::PNG,
        properties: &*empty
    ];
    release_obj(bitmap_rep);

    if png_data.is_null() {
        return Err("PNG 编码失败".into());
    }

    let len: usize = msg_send![png_data, length];
    let ptr: *const u8 = msg_send![png_data, bytes];
    let bytes = std::slice::from_raw_parts(ptr, len).to_vec();
    if bytes.is_empty() {
        return Err("PNG 数据为空".into());
    }
    Ok(bytes)
}

/// CGImage → JPEG 字节（ImageIO，线程安全；质量 0~1）。
///
/// 云端移除背景的输入压缩：Bria 仅接受 JPEG / PNG / WEBP 且限 25MB，
/// 大图与 HEIC 等格式统一转 JPEG 后上传（背景移除不需要输入透明通道）。
pub unsafe fn encode_jpeg(cg_image: *mut c_void, quality: f64) -> Result<Vec<u8>, String> {
    if cg_image.is_null() {
        return Err("CGImage 为空".into());
    }

    let out_data = CFDataCreateMutable(std::ptr::null_mut(), 0);
    if out_data.is_null() {
        return Err("CFDataCreateMutable 失败".into());
    }
    let cstr = std::ffi::CString::new("public.jpeg").map_err(|e| e.to_string())?;
    // kCFStringEncodingUTF8 = 0x08000100
    let uti = CFStringCreateWithCString(std::ptr::null_mut(), cstr.as_ptr(), 0x08000100);
    if uti.is_null() {
        CFRelease(out_data);
        return Err("CFString 创建失败".into());
    }
    let dst = CGImageDestinationCreateWithData(out_data, uti, 1, std::ptr::null_mut());
    CFRelease(uti);
    if dst.is_null() {
        CFRelease(out_data);
        return Err("CGImageDestination 创建失败".into());
    }

    let key_cstr = std::ffi::CString::new("kCGImageDestinationLossyCompressionQuality").unwrap();
    let key = CFStringCreateWithCString(std::ptr::null_mut(), key_cstr.as_ptr(), 0x08000100);
    let q = quality.clamp(0.0, 1.0);
    // kCFNumberDoubleType = 13
    let val = CFNumberCreate(std::ptr::null_mut(), 13, &q as *const f64 as *const c_void);
    let mut props: *mut c_void = std::ptr::null_mut();
    if !key.is_null() && !val.is_null() {
        let keys: [*const c_void; 1] = [key];
        let vals: [*const c_void; 1] = [val];
        props = CFDictionaryCreate(
            std::ptr::null_mut(),
            keys.as_ptr(),
            vals.as_ptr(),
            1,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        );
    }
    if !key.is_null() {
        CFRelease(key);
    }
    if !val.is_null() {
        CFRelease(val);
    }

    CGImageDestinationAddImage(dst, cg_image, props);
    if !props.is_null() {
        CFRelease(props);
    }
    let ok = CGImageDestinationFinalize(dst);
    CFRelease(dst);
    if !ok {
        CFRelease(out_data);
        return Err("JPEG 编码失败".into());
    }

    let len = CFDataGetLength(out_data) as usize;
    let ptr = CFDataGetBytePtr(out_data);
    if ptr.is_null() || len == 0 {
        CFRelease(out_data);
        return Err("JPEG 编码结果为空".into());
    }
    let bytes = std::slice::from_raw_parts(ptr, len).to_vec();
    CFRelease(out_data);
    Ok(bytes)
}

/// 从 PNG 字节读取像素尺寸（云端接口只回图片 URL，不经解码直接取 IHDR）。
pub fn png_dimensions(bytes: &[u8]) -> Result<(u32, u32), String> {
    const PNG_SIG: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 24 || &bytes[..8] != PNG_SIG || &bytes[12..16] != b"IHDR" {
        return Err("云端返回的不是有效 PNG".into());
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    if width == 0 || height == 0 {
        return Err("云端返回的 PNG 尺寸无效".into());
    }
    Ok((width, height))
}

/// 构建处理结果：PNG 字节 → 写临时文件 + base64 data URL。
pub fn build_result(png_bytes: Vec<u8>, width: u32, height: u32) -> Result<ImageResult, String> {
    let temp_path = write_temp_png(&png_bytes)?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
    Ok(ImageResult {
        preview_data_url: format!("data:image/png;base64,{b64}"),
        temp_path,
        width,
        height,
        size_bytes: png_bytes.len() as u64,
    })
}

/// 将 PNG 字节写入临时文件（voidnix_image_ 前缀，启动期自动清理）。
pub fn write_temp_png(bytes: &[u8]) -> Result<String, String> {
    let id = temp_id();
    let path = std::env::temp_dir().join(format!("voidnix_image_{id}.png"));
    crate::runtime::storage::save_png_safely(&path, bytes)?;
    Ok(path.to_string_lossy().into_owned())
}

/// 轻量唯一标识（纳秒时间戳，不引入 uuid crate）。
fn temp_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:032x}")
}

/// 释放 ObjC 对象（等价于 [obj release]）。
pub unsafe fn release_obj(obj: *mut AnyObject) {
    if !obj.is_null() {
        let _: () = msg_send![obj, release];
    }
}

/// 释放 CoreFoundation 对象（CVPixelBuffer / CGColorSpace / CGContext 等）。
pub unsafe fn release_cf(cf: *mut c_void) {
    if !cf.is_null() {
        CFRelease(cf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALPHA_PREMULTIPLIED_LAST: u32 = 1;

    extern "C" {
        fn CGColorSpaceCreateDeviceRGB() -> *mut c_void;
        fn CGBitmapContextCreate(
            data: *mut c_void,
            width: usize,
            height: usize,
            bits_per_component: usize,
            bytes_per_row: usize,
            space: *mut c_void,
            bitmap_info: u32,
        ) -> *mut c_void;
        fn CGContextSetRGBFillColor(ctx: *mut c_void, r: f64, g: f64, b: f64, a: f64);
        fn CGContextFillRect(ctx: *mut c_void, rect: objc2_foundation::NSRect);
        fn CGBitmapContextCreateImage(ctx: *mut c_void) -> *mut c_void;
    }

    /// 云端上传依赖的 JPEG 编码器：跑一次真实 ImageIO 路径（非仅编译）。
    #[test]
    fn encode_jpeg_produces_valid_jpeg() {
        unsafe {
            objc2::rc::autoreleasepool(|_| {
                let color_space = CGColorSpaceCreateDeviceRGB();
                assert!(!color_space.is_null());
                let ctx = CGBitmapContextCreate(
                    std::ptr::null_mut(),
                    8,
                    8,
                    8,
                    0,
                    color_space,
                    ALPHA_PREMULTIPLIED_LAST,
                );
                release_cf(color_space);
                assert!(!ctx.is_null());

                let rect = objc2_foundation::NSRect {
                    origin: objc2_foundation::NSPoint { x: 0.0, y: 0.0 },
                    size: objc2_foundation::NSSize {
                        width: 8.0,
                        height: 8.0,
                    },
                };
                CGContextSetRGBFillColor(ctx, 1.0, 0.0, 0.0, 1.0);
                CGContextFillRect(ctx, rect);
                let cg_image = CGBitmapContextCreateImage(ctx);
                release_cf(ctx);
                assert!(!cg_image.is_null());

                let jpeg = encode_jpeg(cg_image, 0.9).expect("JPEG encode");
                CFRelease(cg_image);
                assert!(jpeg.len() > 4);
                assert_eq!(&jpeg[..2], &[0xFF, 0xD8]);
                assert_eq!(&jpeg[jpeg.len() - 2..], &[0xFF, 0xD9]);
            });
        }
    }
}
