use std::process::Command;

use super::crop::{crop_cg_with_annotation, crop_with_annotation};
use super::ffi::{decode_image_data, get_cg_image, OcrResult, TextRegion};

/// Vision 进程内识别（经 objc2-vision）。
/// 原实现 swift -e 子进程：每次 OCR 启动 Swift 解释器 + 前端编译脚本 + 冷加载
/// Vision/AppKit framework（实测 0.3s 起步，冷缓存更久），叠加 PNG 编码落盘/解码往返。
/// 进程内以 CGImage 直接喂 VNImageRequestHandler，零子进程零落盘零往返，
/// 识别参数（级别/纠错/语言）与原 swift 脚本逐项一致，结果不变。
#[cfg(target_os = "macos")]
mod vision {
    use objc2::rc::Retained;
    use objc2::AnyThread;
    use objc2_core_graphics::CGImage;
    use objc2_foundation::{NSArray, NSDictionary, NSString};
    use objc2_vision::{
        VNDetectBarcodesRequest, VNImageRequestHandler, VNRecognizeTextRequest, VNRequest,
        VNRequestTextRecognitionLevel,
    };

    use super::super::ffi::{OcrResult, TextRegion};

    extern "C" {
        fn CGImageGetWidth(image: *mut std::ffi::c_void) -> usize;
        fn CGImageGetHeight(image: *mut std::ffi::c_void) -> usize;
    }

    /// CGImage 为 CF ZST（引用即裸指针别名）；借用期间 caller 持有引用不释放
    unsafe fn as_cgimage(cg: *mut std::ffi::c_void) -> &'static CGImage {
        unsafe { &*(cg as *const CGImage) }
    }

    fn make_text_request(uses_correction: bool) -> Retained<VNRecognizeTextRequest> {
        let req = VNRecognizeTextRequest::new();
        req.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
        req.setUsesLanguageCorrection(uses_correction);
        let langs = NSArray::from_retained_slice(&[
            NSString::from_str("zh-Hans"),
            NSString::from_str("zh-Hant"),
            NSString::from_str("en-US"),
            NSString::from_str("ja"),
        ]);
        req.setRecognitionLanguages(&langs);
        req
    }

    unsafe fn perform(
        cg: *mut std::ffi::c_void,
        requests: &NSArray<VNRequest>,
    ) -> Result<(), String> {
        // SAFETY: alloc 为 AnyThread 提供方法（VNImageRequestHandler 非主线程限定）；
        // as_cgimage 借用期间 caller 持有引用；options 空字典类型正确
        let handler = unsafe {
            VNImageRequestHandler::initWithCGImage_options(
                VNImageRequestHandler::alloc(),
                as_cgimage(cg),
                &NSDictionary::new(),
            )
        };
        handler
            .performRequests_error(requests)
            .map_err(|e| format!("Vision 识别失败: {e}"))
    }

    /// 文字 + 二维码识别（参数对应原 swift 脚本：accurate / 纠错开 / 四语言）
    pub(super) unsafe fn recognize(cg: *mut std::ffi::c_void) -> Result<OcrResult, String> {
        let text_req = make_text_request(true);
        let qr_req = VNDetectBarcodesRequest::new();
        // SAFETY: cast_unchecked 为上转方向（具体请求类→VNRequest，编译期已知继承链），
        // clone 保留原 typed 句柄供取结果（ObjC 引用语义，同一对象）
        let requests = NSArray::from_retained_slice(&[
            unsafe { Retained::cast_unchecked::<VNRequest>(text_req.clone()) },
            unsafe { Retained::cast_unchecked::<VNRequest>(qr_req.clone()) },
        ]);
        unsafe { perform(cg, &requests)? };

        let text = text_req
            .results()
            .map(|obs| {
                obs.iter()
                    .filter_map(|o| {
                        o.topCandidates(1)
                            .firstObject()
                            .map(|c| c.string().to_string())
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        // SAFETY: payloadStringValue 为 VNBarcodeObservation 标准只读选择子
        let qr = qr_req
            .results()
            .map(|obs| {
                obs.iter()
                    .filter_map(|b| unsafe { b.payloadStringValue() }.map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        Ok(OcrResult { text, qr })
    }

    /// 全图文字区域检测（参数对应原 swift 脚本：accurate / 纠错关 / 四语言）
    pub(super) unsafe fn detect_regions(
        cg: *mut std::ffi::c_void,
        scale: f64,
    ) -> Result<Vec<TextRegion>, String> {
        let req = make_text_request(false);
        // SAFETY: 同 recognize 的上转 + clone 保留
        let requests = NSArray::from_retained_slice(&[unsafe {
            Retained::cast_unchecked::<VNRequest>(req.clone())
        }]);
        unsafe { perform(cg, &requests)? };

        // SAFETY: CGImageGetWidth/Height 为 CoreGraphics C API，入参 caller 已保证有效
        let img_w = unsafe { CGImageGetWidth(cg) } as f64;
        let img_h = unsafe { CGImageGetHeight(cg) } as f64;
        let regions = req
            .results()
            .map(|obs| {
                obs.iter()
                    // SAFETY: boundingBox 为 VNObservation 标准只读选择子
                    .map(|o| {
                        let bb = unsafe { o.boundingBox() };
                        region_from_parts(
                            bb.origin.x,
                            bb.origin.y,
                            bb.size.width,
                            bb.size.height,
                            img_w,
                            img_h,
                            scale,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(regions)
    }

    /// Vision boundingBox（归一化、左下原点）→ 屏幕逻辑坐标（左上原点、物理像素除 scale）
    fn region_from_parts(
        nx: f64,
        ny: f64,
        nw: f64,
        nh: f64,
        img_w: f64,
        img_h: f64,
        scale: f64,
    ) -> TextRegion {
        let x_px = nx * img_w;
        let w_px = nw * img_w;
        let h_px = nh * img_h;
        let y_top = img_h - ny * img_h - h_px;
        TextRegion {
            x: x_px / scale,
            y: y_top / scale,
            w: w_px / scale,
            h: h_px / scale,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn region_convert_full_image_maps_to_viewport() {
            // 全图归一化框 (0,0,1,1) → 逻辑 (0,0,img/scale)
            let r = region_from_parts(0.0, 0.0, 1.0, 1.0, 2000.0, 1000.0, 2.0);
            assert_eq!((r.x, r.y, r.w, r.h), (0.0, 0.0, 1000.0, 500.0));
        }

        #[test]
        fn region_convert_flips_y_axis() {
            // 顶部四分之一（归一化 y=0.75 高 0.25，左下原点）→ 逻辑 y=0 起的高 125
            let r = region_from_parts(0.5, 0.75, 0.25, 0.25, 2000.0, 1000.0, 2.0);
            assert_eq!((r.x, r.y, r.w, r.h), (500.0, 0.0, 250.0, 125.0));
        }
    }
}

#[tauri::command]
pub async fn ocr_image(
    sel_x: f64,
    sel_y: f64,
    sel_w: f64,
    sel_h: f64,
    scale: f64,
    annotation_png: String,
) -> Result<OcrResult, String> {
    let ann = if annotation_png.is_empty() {
        None
    } else {
        Some(decode_image_data(&annotation_png)?)
    };

    #[cfg(target_os = "macos")]
    {
        let cg = crop_cg_with_annotation(sel_x, sel_y, sel_w, sel_h, scale, ann.as_deref())?;
        // SAFETY: cg 为 Create 规则 CGImageRef；recognize 借用期内不释放
        let result = unsafe { vision::recognize(cg) };
        // SAFETY: cg 已过借用期，按 Create 规则释放
        unsafe { super::ffi::CGImageRelease(cg) };
        result
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (ann, sel_x, sel_y, sel_w, sel_h, scale);
        Err("仅支持 macOS".to_string())
    }
}

#[tauri::command]
pub async fn detect_text_regions(scale: f64) -> Result<Vec<TextRegion>, String> {
    #[cfg(target_os = "macos")]
    {
        // 直接消费截屏会话的原始 CGImage（原实现绕道 picker.jpg 磁盘解码——
        // swift 进程读不了本进程内存才落盘；进程内零磁盘往返且取的是原图）
        let raw = get_cg_image();
        if raw.is_null() {
            return Err("无截屏数据".to_string());
        }
        // SAFETY: 会话 CGImage 借用期间 Retain 防并发替换释放
        unsafe { super::ffi::CGImageRetain(raw) };
        // SAFETY: raw 已 Retain + null 检查；detect_regions 借用期内不释放
        let result = unsafe { vision::detect_regions(raw, scale) };
        // SAFETY: 配对释放上方 Retain
        unsafe { super::ffi::CGImageRelease(raw) };
        result
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = scale;
        Err("仅支持 macOS".to_string())
    }
}

#[tauri::command]
pub async fn save_screenshot(
    sel_x: f64,
    sel_y: f64,
    sel_w: f64,
    sel_h: f64,
    scale: f64,
    annotation_png: String,
    path: String,
) -> Result<String, String> {
    let ann = if annotation_png.is_empty() {
        None
    } else {
        Some(decode_image_data(&annotation_png)?)
    };
    #[cfg(target_os = "macos")]
    {
        let png = crop_with_annotation(sel_x, sel_y, sel_w, sel_h, scale, ann.as_deref())?;
        let file_path = {
            let p = std::path::Path::new(&path);
            if p.is_dir() {
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                p.join(format!("screenshot_{}.png", ts))
            } else {
                p.to_path_buf()
            }
        };
        crate::runtime::storage::save_png_safely(&file_path, &png)?;
        Ok(file_path.to_string_lossy().to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (ann, sel_x, sel_y, sel_w, sel_h, scale, path);
        Err("仅支持 macOS".to_string())
    }
}

#[tauri::command]
pub async fn copy_screenshot_to_clipboard(
    sel_x: f64,
    sel_y: f64,
    sel_w: f64,
    sel_h: f64,
    scale: f64,
    annotation_png: String,
) -> Result<(), String> {
    let ann = if annotation_png.is_empty() {
        None
    } else {
        Some(decode_image_data(&annotation_png)?)
    };
    #[cfg(target_os = "macos")]
    {
        let png = crop_with_annotation(sel_x, sel_y, sel_w, sel_h, scale, ann.as_deref())?;
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let tmp = std::env::temp_dir().join(format!("voidnix_clip_{}.png", ts));
        // TempHandle RAII：函数退出（含错误路径）自动清理
        let _tmp_handle = crate::runtime::storage::TempHandle::new(tmp.clone());
        std::fs::write(&tmp, &png).map_err(|e| e.to_string())?;
        let script = format!(
            "set f to POSIX file \"{}\"\nset the clipboard to (read f as «class PNGf»)",
            tmp.display()
        );
        let out = Command::new("osascript")
            .args(["-e", &script])
            .output()
            .map_err(|e| e.to_string())?;
        // tmp 由 _tmp_handle Drop 清理
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (ann, sel_x, sel_y, sel_w, sel_h, scale);
        Err("仅支持 macOS".to_string())
    }
}
