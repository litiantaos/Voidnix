//! 云端移除背景：Bria RMBG-2.0。
//!
//! 本地 Vision 对毛发、半透明边缘与复杂前景的分割质量弱于专用模型；
//! Bria 的 `remove_background` 即 RMBG-2.0（与 remove.bg 同级的商用模型），
//! 新账户含 100 次免费额度，之后 $0.018/张。
//!
//! 管道：本地图片 →（必要时缩放/转 JPEG）→ base64 →
//! POST /v2/image/edit/remove_background（sync）→ image_url → 下载 PNG →
//! 临时文件 + base64 预览。
//!
//! Token 由前端配置经命令参数传入（明文存本地扩展 config，与 AI 提供商同范式）。

use base64::Engine;
use serde::Deserialize;
use std::ffi::c_void;
use std::path::Path;

use super::shared::{self, ImageResult};

const ENDPOINT: &str = "https://engine.prod.bria-api.com/v2/image/edit/remove_background";

/// 云端输入像素上限（保守值，Bria 大图上限随套餐；超出缩小以免 406）。
const MAX_INPUT_PIXELS: u64 = 20_000_000;
/// 超过此大小先重编码 JPEG（不降分辨率也能少传 30–50%）。
const RECOMPRESS_BYTES: usize = 2 * 1024 * 1024;
/// 上传 JPEG 质量（抠图输入不需要更高，体积换速度）。
const JPEG_QUALITY: f64 = 0.9;

/// kCGImageAlphaPremultipliedLast
const ALPHA_PREMULTIPLIED_LAST: u32 = 1;

#[derive(Deserialize)]
struct BriaResponse {
    result: Option<BriaResult>,
}

#[derive(Deserialize)]
struct BriaResult {
    image_url: Option<String>,
}

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
    fn CGContextDrawImage(ctx: *mut c_void, rect: objc2_foundation::NSRect, image: *mut c_void);
    fn CGContextSetRGBFillColor(ctx: *mut c_void, r: f64, g: f64, b: f64, a: f64);
    fn CGContextFillRect(ctx: *mut c_void, rect: objc2_foundation::NSRect);
    fn CGBitmapContextCreateImage(ctx: *mut c_void) -> *mut c_void;
    fn CGImageRelease(image: *mut c_void);
}

/// 调用 Bria 移除背景。调用方持有图片处理互斥锁。
pub async fn remove_background(
    input_path: &str,
    token: &str,
    max_edge: u32,
    progress: &tauri::ipc::Channel<String>,
) -> Result<ImageResult, String> {
    let token = token.trim();
    if token.is_empty() {
        return Err("未配置 Bria API Token（图片处理 → 设置）".into());
    }

    let _ = progress.send("preparing".to_string());
    let started = std::time::Instant::now();
    let path = input_path.to_string();
    let prepared = tokio::task::spawn_blocking(move || {
        objc2::rc::autoreleasepool(|_| prepare_input(&path, max_edge))
    })
    .await
    .map_err(|e| format!("图片预处理异常: {e}"))??;
    let prep_ms = started.elapsed().as_millis();

    let body = serde_json::json!({
        "image": base64::engine::general_purpose::STANDARD.encode(&prepared),
        "sync": true,
        "preserve_alpha": true,
    });
    let payload_kib = prepared.len().div_ceil(1024) * 4 / 3;

    let _ = progress.send("uploading".to_string());
    let request_started = std::time::Instant::now();
    let response = crate::http::client()
        .post(ENDPOINT)
        .header("api_token", token)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("请求 Bria 失败: {e}"))?;
    let request_ms = request_started.elapsed().as_millis();

    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| format!("读取 Bria 响应失败: {e}"))?;
    if !status.is_success() {
        return Err(api_error(status.as_u16(), &text));
    }

    let parsed: BriaResponse =
        serde_json::from_str(&text).map_err(|e| format!("解析 Bria 响应失败: {e}"))?;
    let url = parsed
        .result
        .and_then(|r| r.image_url)
        .ok_or_else(|| "Bria 未返回图片地址".to_string())?;

    // 结果 URL 由外部服务返回：仅接受 https + 公网 host（防 SSRF 链式跳转）
    let is_https = url
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"));
    if !is_https {
        return Err("Bria 返回的图片地址不安全（非 HTTPS）".into());
    }
    crate::http::validate_url(&url)?;

    let _ = progress.send("downloading".to_string());
    let download_started = std::time::Instant::now();
    let image = crate::http::stream_client()
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("下载 Bria 结果失败: {e}"))?;
    if !image.status().is_success() {
        return Err(format!("下载 Bria 结果失败（HTTP {}）", image.status()));
    }
    let image_wait_ms = download_started.elapsed().as_millis();
    let bytes = image
        .bytes()
        .await
        .map_err(|e| format!("读取 Bria 结果失败: {e}"))?
        .to_vec();
    let download_ms = download_started.elapsed().as_millis();

    let (width, height) = shared::png_dimensions(&bytes)?;
    eprintln!(
        "[image:bria] edge={max_edge} in={}KiB b64≈{}KiB prep={prep_ms}ms request={request_ms}ms \
         imgWait={image_wait_ms}ms download={download_ms}ms out={}KiB {}x{} total={}ms",
        prepared.len() / 1024,
        payload_kib,
        bytes.len() / 1024,
        width,
        height,
        started.elapsed().as_millis()
    );
    shared::build_result(bytes, width, height)
}

/// 读取并规范化输入：Bria 只收 JPEG/PNG/WEBP。
///
/// 透传条件：格式受支持 + 未超像素上限 + 无需按 `max_edge` 缩小 + 原始 ≤2MB。
/// 其余统一等比缩放（max_edge / 20MP 双重上限）、白底绘制后 JPEG q0.9；
/// 未缩小且 JPEG 反而更大时退回原始字节（避免重编码膨胀）。
fn prepare_input(input_path: &str, max_edge: u32) -> Result<Vec<u8>, String> {
    let path = Path::new(input_path);
    if !crate::platform::path_guard::validate(path) {
        return Err(format!("路径不安全或不存在：{input_path}"));
    }

    let original = std::fs::read(path).map_err(|e| format!("读取图片失败: {e}"))?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();

    // Native 解码取像素尺寸（HEIC 等无原生扩展名判断）
    let loaded = unsafe { shared::load_image(input_path)? };
    let pixels = loaded.width as u64 * loaded.height as u64;
    let scale = target_scale(loaded.width, loaded.height, max_edge);
    let supported = matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp");

    let passthrough = supported
        && scale >= 0.999
        && pixels <= MAX_INPUT_PIXELS
        && original.len() <= RECOMPRESS_BYTES;
    if passthrough {
        return Ok(original);
    }

    let jpeg = match unsafe { scaled_jpeg(&loaded, scale) } {
        Ok(jpeg) => jpeg,
        // 仅重编码（未缩放）失败时退回原图：格式受支持，Bria 可直收
        Err(_) if supported && scale >= 0.999 => return Ok(original),
        Err(e) => return Err(e),
    };
    if supported && scale >= 0.999 && jpeg.len() >= original.len() {
        return Ok(original);
    }
    Ok(jpeg)
}

/// 计算等比缩放系数：`max_edge`（0 = 不按边长限制）与 20MP 像素上限取更小者。
fn target_scale(width: u32, height: u32, max_edge: u32) -> f64 {
    let pixels = width as u64 * height as u64;
    if pixels == 0 {
        return 1.0;
    }
    let mut scale = 1.0f64;
    if max_edge > 0 {
        let longest = width.max(height) as f64;
        if longest > max_edge as f64 {
            scale = max_edge as f64 / longest;
        }
    }
    if pixels as f64 * scale * scale > MAX_INPUT_PIXELS as f64 {
        scale = scale.min((MAX_INPUT_PIXELS as f64 / pixels as f64).sqrt());
    }
    scale
}

/// 等比缩放到目标比例，白底绘制后 JPEG 编码（透明通道对抠图输入无意义）。
unsafe fn scaled_jpeg(loaded: &shared::Loaded, scale: f64) -> Result<Vec<u8>, String> {
    let width = ((loaded.width as f64 * scale).round() as usize).max(1);
    let height = ((loaded.height as f64 * scale).round() as usize).max(1);

    let color_space = CGColorSpaceCreateDeviceRGB();
    if color_space.is_null() {
        return Err("无法创建色彩空间".into());
    }
    let ctx = CGBitmapContextCreate(
        std::ptr::null_mut(),
        width,
        height,
        8,
        0,
        color_space,
        ALPHA_PREMULTIPLIED_LAST,
    );
    shared::release_cf(color_space);
    if ctx.is_null() {
        return Err("无法创建位图上下文（图片过大？）".into());
    }

    let rect = objc2_foundation::NSRect {
        origin: objc2_foundation::NSPoint { x: 0.0, y: 0.0 },
        size: objc2_foundation::NSSize {
            width: width as f64,
            height: height as f64,
        },
    };
    // 白底：JPEG 无透明通道，避免 PNG 透明区域转黑
    CGContextSetRGBFillColor(ctx, 1.0, 1.0, 1.0, 1.0);
    CGContextFillRect(ctx, rect);
    CGContextDrawImage(ctx, rect, loaded.cg_image);

    let cg_out = CGBitmapContextCreateImage(ctx);
    shared::release_cf(ctx);
    if cg_out.is_null() {
        return Err("图片缩放失败".into());
    }
    let jpeg = shared::encode_jpeg(cg_out, JPEG_QUALITY);
    CGImageRelease(cg_out);
    jpeg
}

/// HTTP 错误 → 可读文案（提取 Bria 错误契约里的 details / message）。
fn api_error(status: u16, body: &str) -> String {
    let detail = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            let err = v.get("error")?;
            err.get("details")
                .and_then(|d| d.as_str())
                .or_else(|| err.get("message").and_then(|m| m.as_str()))
                .map(str::to_string)
        })
        .unwrap_or_else(|| body.chars().take(200).collect());

    let hint = match status {
        401 => "Token 无效或已失效",
        402 | 403 => "额度不足或无权使用该接口",
        406 | 413 => "图片过大",
        415 => "图片格式不受支持",
        429 => "请求过于频繁，请稍后再试",
        500..=599 => "Bria 服务暂时不可用",
        _ => "请求失败",
    };
    format!("Bria {hint}（HTTP {status}）：{detail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_error_uses_details_and_hint() {
        let body = r#"{"error":{"code":401,"message":"invalid_api_token","details":"Api token provided is invalid"},"request_id":"x"}"#;
        let msg = api_error(401, body);
        assert!(msg.contains("Token"));
        assert!(msg.contains("Api token provided is invalid"));
    }

    #[test]
    fn api_error_falls_back_to_body() {
        let msg = api_error(500, "upstream exploded");
        assert!(msg.contains("upstream exploded"));
        assert!(msg.contains("HTTP 500"));
    }

    #[test]
    fn png_dimensions_reads_ihdr() {
        let mut png = Vec::new();
        png.extend_from_slice(b"\x89PNG\r\n\x1a\n");
        png.extend_from_slice(&[0, 0, 0, 13]); // IHDR length
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&640u32.to_be_bytes());
        png.extend_from_slice(&480u32.to_be_bytes());
        assert_eq!(shared::png_dimensions(&png), Ok((640, 480)));
    }

    #[test]
    fn png_dimensions_rejects_non_png() {
        assert!(shared::png_dimensions(b"not a png").is_err());
    }

    #[test]
    fn target_scale_caps_long_edge() {
        let scale = target_scale(4032, 3024, 2048);
        assert!((scale - 2048.0 / 4032.0).abs() < 1e-9);
    }

    #[test]
    fn target_scale_keeps_small_images() {
        assert_eq!(target_scale(1024, 768, 2048), 1.0);
        assert_eq!(target_scale(4032, 3024, 0), 1.0);
    }

    #[test]
    fn target_scale_applies_pixel_cap_without_edge_limit() {
        let scale = target_scale(8000, 6000, 0);
        let pixels = 8000.0 * scale * 6000.0 * scale;
        assert!(pixels <= MAX_INPUT_PIXELS as f64 + 1.0);
        assert!(pixels > MAX_INPUT_PIXELS as f64 * 0.99);
    }
}
