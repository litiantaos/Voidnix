use super::ffi::{get_cg_image, CGImageRelease};

#[cfg(target_os = "macos")]
pub(super) fn crop_with_annotation(
    sel_x: f64,
    sel_y: f64,
    sel_w: f64,
    sel_h: f64,
    scale: f64,
    annotation_png: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    use core_graphics::geometry::{CGPoint, CGRect, CGSize};
    use objc2::runtime::AnyObject;

    let raw = get_cg_image();
    if raw.is_null() {
        return Err("无截屏数据".to_string());
    }

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
    let cropped = unsafe { CGImageCreateWithImageInRect(raw, rect) };
    if cropped.is_null() {
        return Err("CGImageCreateWithImageInRect 失败".to_string());
    }

    let result = unsafe {
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

        let props_cls = objc2::class!(NSDictionary);
        let props: *mut AnyObject = objc2::msg_send![props_cls, dictionary];
        let ns_data: *mut AnyObject =
            objc2::msg_send![rep, representationUsingType: 4usize, properties: props];
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
