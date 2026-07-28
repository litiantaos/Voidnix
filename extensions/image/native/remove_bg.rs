//! 图片背景移除：macOS Vision 框架前景实例分割。
//!
//! 使用 VNGenerateForegroundInstanceMaskRequest（macOS 14+ 内置模型，
//! 与 Photos「抬起主体」同一引擎），对任意前景物体（人/物/动物）生成高质量分割。
//! 零外部依赖：模型内置于系统，无需下载；GPU 加速，通常 <1s。
//!
//! 管道：NSImage 加载 → CGImage → VNImageRequestHandler →
//! VNGenerateForegroundInstanceMaskRequest → VNInstanceMaskObservation →
//! generateMaskedImageOfInstances（背景已置透明黑 CVPixelBuffer）→
//! CIImage 渲染 → PNG 编码。

use objc2::rc::autoreleasepool;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
use objc2_foundation::{NSDictionary, NSString};
use std::ffi::c_void;

use super::shared::{self, ExtractedImage};

// CoreVideo CVPixelBuffer（仅取宽高，像素读取走 CIImage 路径）

#[link(name = "CoreVideo", kind = "framework")]
extern "C" {
    fn CVPixelBufferGetWidth(pb: *mut c_void) -> usize;
    fn CVPixelBufferGetHeight(pb: *mut c_void) -> usize;
}

extern "C" {
    fn CGImageRelease(image: *mut c_void);
}

/// 执行背景移除。
///
/// 在 autoreleasepool 内执行所有 Objective-C 操作。调用方应在 `spawn_blocking` 中调用。
pub fn remove_background(input_path: &str) -> Result<shared::ImageResult, String> {
    autoreleasepool(|_| -> Result<shared::ImageResult, String> {
        let extracted = unsafe { run_vision_pipeline(input_path)? };
        shared::build_result(extracted.png_bytes, extracted.width, extracted.height)
    })
}

/// Vision 管道核心：alloc+init 对象在返回前逐一 release。
///
/// 错误路径上仅便捷构造函数对象（自动释放）泄漏至 autoreleasepool 回收；
/// alloc+init 对象在对应错误分支前尚未创建，无需释放。
unsafe fn run_vision_pipeline(input_path: &str) -> Result<ExtractedImage, String> {
    // ── 1. 加载图片（Loaded 持有 NSImage，保持 CGImage 有效）──
    let loaded = shared::load_image(input_path)?;

    // ── 2. 创建 VNImageRequestHandler（alloc+init，需 release）──
    let empty_dict = NSDictionary::<NSString, AnyObject>::new();
    let handler: *mut AnyObject = msg_send![class!(VNImageRequestHandler), alloc];
    let handler: *mut AnyObject =
        msg_send![handler, initWithCGImage: loaded.cg_image, options: &*empty_dict];

    // ── 3. 创建并执行分割请求（alloc+init，需 release）──
    let request: *mut AnyObject = msg_send![class!(VNGenerateForegroundInstanceMaskRequest), alloc];
    let request: *mut AnyObject = msg_send![request, init];

    let requests_arr: *mut AnyObject = msg_send![class!(NSArray), arrayWithObject: request];
    let mut error: *mut AnyObject = std::ptr::null_mut();
    let success: bool = msg_send![handler, performRequests: requests_arr, error: &mut error];
    if !success {
        let msg = extract_error_message(error);
        shared::release_obj(handler);
        shared::release_obj(request);
        return Err(msg);
    }

    // ── 4. 获取 VNInstanceMaskObservation ──
    let results: *mut AnyObject = msg_send![request, results];
    let count: usize = msg_send![results, count];
    if count == 0 {
        shared::release_obj(handler);
        shared::release_obj(request);
        return Err("未检测到前景物体".into());
    }
    let observation: *mut AnyObject = msg_send![results, firstObject];

    // ── 5. 生成带透明背景的图像（CF_RETURNS_RETAINED，需 release）──
    let all_instances: *mut AnyObject = msg_send![observation, allInstances];
    let mut mask_error: *mut AnyObject = std::ptr::null_mut();
    let masked_pb: *mut c_void = msg_send![
        observation,
        generateMaskedImageOfInstances: all_instances,
        fromRequestHandler: handler,
        croppedToInstancesExtent: 0u8,
        error: &mut mask_error
    ];
    if masked_pb.is_null() {
        let msg = extract_error_message(mask_error);
        shared::release_obj(handler);
        shared::release_obj(request);
        return Err(msg);
    }

    let width = CVPixelBufferGetWidth(masked_pb) as u32;
    let height = CVPixelBufferGetHeight(masked_pb) as u32;

    // ── 6. CVPixelBuffer → CIImage → CGImage ──
    let ci_image: *mut AnyObject = msg_send![class!(CIImage), imageWithCVPixelBuffer: masked_pb];
    let ci_context: *mut AnyObject = msg_send![class!(CIContext), context];
    let extent: objc2_foundation::NSRect = msg_send![ci_image, extent];
    let cg_out: *mut c_void = msg_send![ci_context, createCGImage: ci_image, fromRect: extent];

    // masked_pb 已转交给 CIImage，释放
    shared::release_cf(masked_pb);

    if cg_out.is_null() {
        shared::release_obj(handler);
        shared::release_obj(request);
        return Err("CIImage 渲染失败".into());
    }

    // ── 7. CGImage → PNG ──
    let png_result = shared::encode_png(cg_out);
    CGImageRelease(cg_out);
    shared::release_obj(handler);
    shared::release_obj(request);

    png_result.map(|png_bytes| ExtractedImage {
        png_bytes,
        width,
        height,
    })
}

/// 从 NSError * 提取本地化描述。
unsafe fn extract_error_message(error: *mut AnyObject) -> String {
    if error.is_null() {
        return "未知错误".into();
    }
    let desc: *mut NSString = msg_send![error, localizedDescription];
    if desc.is_null() {
        return "未知错误".into();
    }
    (*desc).to_string()
}
