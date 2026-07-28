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
