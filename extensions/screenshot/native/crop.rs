use super::ffi::{get_cg_image, CGImageRelease, CGImageRetain};

#[cfg(target_os = "macos")]
/// 裁剪选区 + 合成标注，返回绘制完成的 NSBitmapImageRep（caller release）。
/// PNG 路径（copy/save）与 CGImage 路径（OCR 喂 Vision）共用此合成步骤。
pub(super) unsafe fn compose_annotated_rep(
    sel_x: f64,
    sel_y: f64,
    sel_w: f64,
    sel_h: f64,
    scale: f64,
    annotation_png: Option<&[u8]>,
) -> Result<*mut objc2::runtime::AnyObject, String> {
    use core_graphics::geometry::{CGPoint, CGRect, CGSize};
    use objc2::runtime::AnyObject;

    let raw = get_cg_image();
    if raw.is_null() {
        return Err("无截屏数据".to_string());
    }
    // SAFETY: raw 已 null 检查；借用期间 Retain 防合成期间 store_cg_image 换图释放
    // 会话 CGImage（与 detect_text_regions 同防护），各出口（含错误路径）配对 Release
    unsafe { CGImageRetain(raw) };

    extern "C" {
        fn CGImageCreateWithImageInRect(
            image: *mut std::ffi::c_void,
            rect: CGRect,
        ) -> *mut std::ffi::c_void;
    }
    let rect = CGRect {
        origin: CGPoint {
            x: sel_x * scale,
            y: sel_y * scale,
        },
        size: CGSize {
            width: sel_w * scale,
            height: sel_h * scale,
        },
    };
    // SAFETY: raw 已上方 null 检查；CGImageCreateWithImageInRect 为 CoreGraphics C API，
    // rect 为合法 CGRect（坐标已 scale 缩放），返回 Create 规则 CGImageRef（caller release）
    let cropped = unsafe { CGImageCreateWithImageInRect(raw, rect) };
    if cropped.is_null() {
        // SAFETY: 配对释放上方对 raw 的 Retain
        unsafe { CGImageRelease(raw) };
        return Err("CGImageCreateWithImageInRect 失败".to_string());
    }

    // SAFETY: cropped 已 null 检查；alloc/initWithBitmapDataPlanes:/setSize:/drawInRect:/
    // release 均为 NSBitmapImageRep/NSImage/NSGraphicsContext/NSDictionary 标准选择子，
    // 参数类型匹配；返回 rep 为 caller release（错误路径释放 cropped 与 raw 的 Retain）
    unsafe {
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
            CGImageRelease(raw);
            return Err("NSBitmapImageRep alloc 失败".to_string());
        }

        let _: () = objc2::msg_send![
            rep,
            setSize: objc2_foundation::NSSize::new(sel_w, sel_h)
        ];

        let gc_cls = objc2::class!(NSGraphicsContext);
        let gc: *mut AnyObject = objc2::msg_send![gc_cls, graphicsContextWithBitmapImageRep: rep];
        let _: () = objc2::msg_send![gc_cls, saveGraphicsState];
        let _: () = objc2::msg_send![gc_cls, setCurrentContext: gc];

        let ns_image_cls = objc2::class!(NSImage);
        let bg_img: *mut AnyObject = objc2::msg_send![ns_image_cls, alloc];
        let bg_img: *mut AnyObject = objc2::msg_send![
            bg_img,
            initWithCGImage: cropped,
            size: objc2_foundation::NSSize::new(sel_w, sel_h)
        ];
        if !bg_img.is_null() {
            let dst = objc2_foundation::NSRect::new(
                objc2_foundation::NSPoint::new(0.0, 0.0),
                objc2_foundation::NSSize::new(sel_w, sel_h),
            );
            let _: () = objc2::msg_send![bg_img, drawInRect: dst];
            let _: () = objc2::msg_send![bg_img, release];
        }

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
        CGImageRelease(cropped);
        CGImageRelease(raw);
        Ok(rep)
    }
}

#[cfg(target_os = "macos")]
pub(super) fn crop_with_annotation(
    sel_x: f64,
    sel_y: f64,
    sel_w: f64,
    sel_h: f64,
    scale: f64,
    annotation_png: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    use objc2::runtime::AnyObject;

    // SAFETY: autoreleasepool 包住整个合成/编码流程——compose 与本函数内的便捷构造对象
    // （NSGraphicsContext/NSDictionary/NSData）均 autoreleased，worker 线程无 pool 时
    // autorelease 永不执行会累积泄漏（每次调用泄漏一张选区图的内存）；pool 在闭包
    // 结束时 drain 归还。rep 释放前完成 PNG 编码拷贝。
    objc2::rc::autoreleasepool(|_| {
        let rep =
            unsafe { compose_annotated_rep(sel_x, sel_y, sel_w, sel_h, scale, annotation_png)? };

        // SAFETY: rep 已 null 检查；representationUsingType:/dictionary/release 均为
        // NSBitmapImageRep/NSDictionary 标准选择子；ns_data 释放前 length/bytes 拷贝出数据
        let result = unsafe {
            let props_cls = objc2::class!(NSDictionary);
            let props: *mut AnyObject = objc2::msg_send![props_cls, dictionary];
            let ns_data: *mut AnyObject =
                objc2::msg_send![rep, representationUsingType: 4usize, properties: props];
            let _: () = objc2::msg_send![rep, release];

            if ns_data.is_null() {
                return Err("PNG 输出失败".to_string());
            }
            let length: usize = objc2::msg_send![ns_data, length];
            let bytes: *const u8 = objc2::msg_send![ns_data, bytes];
            let png = std::slice::from_raw_parts(bytes, length).to_vec();
            let _: () = objc2::msg_send![ns_data, release];
            png
        };
        Ok(result)
    })
}

#[cfg(target_os = "macos")]
/// 裁剪 + 标注合成的 CGImage 路径（OCR 喂 Vision 直接消费位图，跳过 PNG 编码/落盘/解码）。
/// 返回 Create 规则 CGImageRef（caller CGImageRelease）。
pub(super) fn crop_cg_with_annotation(
    sel_x: f64,
    sel_y: f64,
    sel_w: f64,
    sel_h: f64,
    scale: f64,
    annotation_png: Option<&[u8]>,
) -> Result<*mut std::ffi::c_void, String> {
    // SAFETY: autoreleasepool 同上（rep.CGImage 为 autoreleased，无 pool 时 autorelease
    // 永不执行、图像引用悬置泄漏）；pool drain 后仅存我们的 CGImageRetain（+1 接管，
    // caller 配对 CGImageRelease），rep 随即 release
    unsafe {
        objc2::rc::autoreleasepool(|_| {
            let rep = compose_annotated_rep(sel_x, sel_y, sel_w, sel_h, scale, annotation_png)?;
            let cg: *mut std::ffi::c_void = objc2::msg_send![rep, CGImage];
            let _: () = objc2::msg_send![rep, release];
            if cg.is_null() {
                return Err("CGImage 输出失败".to_string());
            }
            super::ffi::CGImageRetain(cg);
            Ok(cg)
        })
    }
}
