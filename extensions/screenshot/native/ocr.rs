use std::process::Command;

use super::crop::crop_with_annotation;
use super::ffi::{decode_image_data, picker_jpeg_path, OcrResult, TextRegion};

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
        let png = crop_with_annotation(sel_x, sel_y, sel_w, sel_h, scale, ann.as_deref())?;
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let tmp = std::env::temp_dir().join(format!("voidnix_ocr_{}.png", ts));
        // TempHandle RAII：函数退出（含错误路径）自动清理
        let _tmp_handle = crate::runtime::storage::TempHandle::new(tmp.clone());
        std::fs::write(&tmp, &png).map_err(|e| e.to_string())?;

        let script = format!(
            r#"import Vision; import AppKit; import Foundation
let url = URL(fileURLWithPath: "{path}")
guard let img = NSImage(contentsOf: url), let cg = img.cgImage(forProposedRect: nil, context: nil, hints: nil) else {{ print("{{}}"); exit(0) }}
let handler = VNImageRequestHandler(cgImage: cg, options: [:])
let textReq = VNRecognizeTextRequest()
textReq.recognitionLevel = .accurate; textReq.usesLanguageCorrection = true
textReq.recognitionLanguages = ["zh-Hans","zh-Hant","en-US","ja"]
let qrReq = VNDetectBarcodesRequest()
try? handler.perform([textReq, qrReq])
let textResults = (textReq.results ?? []).compactMap {{ $0.topCandidates(1).first?.string }}.joined(separator: "\n")
let qrResults = (qrReq.results ?? []).compactMap {{ $0.payloadStringValue }}
let dict: [String: Any] = ["text": textResults, "qr": qrResults]
let data = try! JSONSerialization.data(withJSONObject: dict, options: [])
print(String(data: data, encoding: .utf8)!)"#,
            path = tmp.display()
        );
        let out = Command::new("swift")
            .args(["-e", &script])
            .output()
            .map_err(|e| format!("swift 失败: {e}"))?;
        // tmp 由 _tmp_handle Drop 清理
        if out.status.success() {
            let json = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let parsed: serde_json::Value =
                serde_json::from_str(&json).map_err(|e| format!("解析识别结果失败: {e}"))?;
            Ok(OcrResult {
                text: parsed
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                qr: parsed
                    .get("qr")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default(),
            })
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

#[tauri::command]
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
        let out = Command::new("swift")
            .args(["-e", &script])
            .output()
            .map_err(|e| format!("swift 失败: {e}"))?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
        }
        let stdout = String::from_utf8_lossy(&out.stdout);
        let mut regions = Vec::new();
        for line in stdout.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() != 4 {
                continue;
            }
            let (Ok(x), Ok(y), Ok(w), Ok(h)) = (
                parts[0].parse::<f64>(),
                parts[1].parse::<f64>(),
                parts[2].parse::<f64>(),
                parts[3].parse::<f64>(),
            ) else {
                continue;
            };
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
