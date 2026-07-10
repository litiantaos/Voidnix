//! CGImage 解码 / 区域捕获 / PNG·JPEG 编码。

use super::super::ffi::voidnix_screenshot_capture_region;
use super::state::SESSION;

pub fn cg_image_to_bgra8(cg: *mut std::ffi::c_void) -> Option<(usize, usize, Vec<u8>)> {
    if cg.is_null() {
        return None;
    }
    // SAFETY: cg 已 null 检查；CGImageGetWidth/Height/ColorSpaceCreateDeviceRGB/
    // CGBitmapContextCreate/DrawImage/Release 均为 CoreGraphics C API。buf.as_mut_ptr()
    // 指向有效缓冲区（vec![0; row_bytes*h]），w/h 非 0 已校验；ctx null 检查后释放。
    // bitmap_info = kCGImageAlphaPremultipliedLast | kCGBitmapByteOrder32Big 合法
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
            size: core_graphics::geometry::CGSize {
                width: w as f64,
                height: h as f64,
            },
        };
        CGContextDrawImage(ctx, rect, cg);
        CGContextRelease(ctx);
        CGColorSpaceRelease(cs);
        Some((w, h, buf))
    }
}

pub fn capture_below_overlay(
    sel_x: f64,
    sel_y: f64,
    sel_w: f64,
    sel_h: f64,
    _overlay_window_id: u32,
) -> Option<(usize, usize, Vec<u8>)> {
    // SAFETY: ns_addr 从 SESSION 读取（非 0 已校验）；voidnix_screenshot_capture_region
    // 写入 out_image（栈指针），null 检查后交 cg_image_to_bgra8 解码，CGImageRelease 配平
    unsafe {
        extern "C" {
            fn CGImageRelease(image: *mut std::ffi::c_void);
        }
        let ns_addr = {
            let g = SESSION.lock().unwrap_or_else(|e| e.into_inner());
            g.as_ref().map(|s| s.ns_window_addr).unwrap_or(0)
        };
        if ns_addr == 0 {
            return None;
        }

        let mut out_image: *mut std::ffi::c_void = std::ptr::null_mut();
        voidnix_screenshot_capture_region(
            ns_addr as *mut std::ffi::c_void,
            sel_x,
            sel_y,
            sel_w,
            sel_h,
            &mut out_image,
        );
        if out_image.is_null() {
            return None;
        }
        let res = cg_image_to_bgra8(out_image);
        CGImageRelease(out_image);
        res
    }
}

pub fn encode_png(buf: &[u8], width: usize, height: usize) -> Result<Vec<u8>, String> {
    encode_image_with_cg(buf, width, height, "public.png", 1.0)
}

pub fn encode_jpeg(
    buf: &[u8],
    width: usize,
    height: usize,
    quality: f64,
) -> Result<Vec<u8>, String> {
    encode_image_with_cg(buf, width, height, "public.jpeg", quality)
}

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
    // SAFETY: buf.len() == width*height*4 已校验；CGColorSpaceCreateDeviceRGB/
    // CGDataProviderCreateWithData/CGImageCreate/CGImageDestination*/CF* 均为 CoreGraphics/
    // CoreFoundation C API。所有 Create 返回值均 null 检查，CFRelease/CGImageRelease 配平；
    // from_raw_parts(bytes, len) 在 out_data release 前拷贝（CFData 缓冲区有效）
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

        let cs = CGColorSpaceCreateDeviceRGB();
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
        let cg_image = CGImageCreate(
            width,
            height,
            8,
            32,
            width * 4,
            cs,
            bitmap_info,
            provider,
            std::ptr::null(),
            false,
            0,
        );
        CGDataProviderRelease(provider);
        CGColorSpaceRelease(cs);
        if cg_image.is_null() {
            return Err("CGImageCreate 失败".to_string());
        }

        let out_data = CFDataCreateMutable(std::ptr::null_mut(), 0);
        if out_data.is_null() {
            CGImageRelease(cg_image);
            return Err("CFDataCreateMutable 失败".to_string());
        }
        let cstr = std::ffi::CString::new(uti).map_err(|e| e.to_string())?;
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

        let mut props_dict: *mut std::ffi::c_void = std::ptr::null_mut();
        if uti == "public.jpeg" {
            let key_cstr =
                std::ffi::CString::new("kCGImageDestinationLossyCompressionQuality").unwrap();
            let key =
                CFStringCreateWithCString(std::ptr::null_mut(), key_cstr.as_ptr(), 0x08000100);
            let q = quality;
            let val = CFNumberCreate(
                std::ptr::null_mut(),
                12,
                &q as *const f64 as *const std::ffi::c_void,
            );
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
        }

        CGImageDestinationAddImage(dst, cg_image, props_dict);
        if !props_dict.is_null() {
            CFRelease(props_dict);
        }
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
