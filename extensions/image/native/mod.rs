//! 图片处理扩展：背景移除（Vision / Bria RMBG-2.0）+ 长图拼接（CoreGraphics）。

mod bria;
mod remove_bg;
mod shared;
mod stitch;

use crate::platform::pasteboard;
use crate::runtime::registry::Extension;
use base64::Engine;
use shared::ImageResult;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use stitch::Direction;
use tauri::AppHandle;

static BUSY: AtomicBool = AtomicBool::new(false);

/// 移除图片背景。
///
/// engine = `vision`（默认，系统内置模型，零依赖）或 `bria`（云端 RMBG-2.0，
/// 需 token，新账户 100 次免费）。Vision 在 spawn_blocking 中执行（performRequests
/// 同步阻塞）；Bria 走异步 HTTP + 本地预处理（max_edge 限制云端输入最长边）。
/// `on_progress` 仅 Bria 使用：preparing / uploading / downloading 三阶段。
#[tauri::command]
pub async fn image_remove_bg(
    input_path: String,
    engine: Option<String>,
    token: Option<String>,
    max_edge: Option<u32>,
    on_progress: tauri::ipc::Channel<String>,
) -> Result<ImageResult, String> {
    let _busy = acquire_busy()?;
    let result = async {
        match engine.as_deref() {
            Some("bria") => {
                bria::remove_background(
                    &input_path,
                    token.as_deref().unwrap_or(""),
                    max_edge.unwrap_or(0),
                    &on_progress,
                )
                .await
            }
            _ => tokio::task::spawn_blocking(move || remove_bg::remove_background(&input_path))
                .await
                .map_err(|e| format!("处理任务异常: {e}"))?,
        }
    }
    .await;
    result
}

/// 拼接长图（横向/纵向，支持间距与重叠、统一尺寸）。
///
/// gap 正值=间距，负值=重叠（如电影截图台词拼接）。
/// resize 控制统一尺寸：none 原始尺寸居中 / width(n) 等比统一宽度 / height(n) 等比统一高度。
/// 在 spawn_blocking 中执行（CoreGraphics 位图合成同步阻塞）。
#[tauri::command]
pub async fn image_stitch(
    input_paths: Vec<String>,
    direction: Direction,
    gap: i64,
    resize: stitch::Resize,
) -> Result<ImageResult, String> {
    let _busy = acquire_busy()?;
    tokio::task::spawn_blocking(move || stitch::stitch(&input_paths, direction, gap, resize))
        .await
        .map_err(|e| format!("处理任务异常: {e}"))?
}

/// 将临时结果文件保存到指定路径。
#[tauri::command]
pub async fn image_save_result(temp_path: String, output_path: String) -> Result<(), String> {
    let temp = PathBuf::from(&temp_path);
    let output = PathBuf::from(&output_path);

    if !crate::platform::path_guard::validate(&temp) {
        return Err("临时文件路径不安全或不存在".into());
    }

    let bytes = tokio::fs::read(&temp)
        .await
        .map_err(|e| format!("读取临时文件失败: {e}"))?;

    crate::runtime::storage::save_png_safely(&output, &bytes)
}

/// 将临时结果复制到剪贴板（PNG 格式，含透明通道）。
#[tauri::command]
pub async fn image_copy_to_clipboard(temp_path: String) -> Result<(), String> {
    let temp = PathBuf::from(&temp_path);
    if !crate::platform::path_guard::validate(&temp) {
        return Err("临时文件路径不安全或不存在".into());
    }
    let bytes = tokio::fs::read(&temp)
        .await
        .map_err(|e| format!("读取临时文件失败: {e}"))?;
    pasteboard::clear();
    pasteboard::set_png_bytes(&bytes);
    Ok(())
}

/// 读取图片文件并返回 data URL（供前端 <img> 预览）。
///
/// 直接 base64 编码原始字节，WKWebView 系统解码器支持 PNG/JPEG/HEIC/WebP/TIFF 等全格式。
#[tauri::command]
pub async fn image_read_preview(input_path: String) -> Result<String, String> {
    let path = PathBuf::from(&input_path);
    if !crate::platform::path_guard::validate(&path) {
        return Err("路径不安全或不存在".into());
    }
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| format!("读取文件失败: {e}"))?;
    let mime = mime_from_ext(&path);
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{mime};base64,{b64}"))
}

/// 从文件扩展名推断 MIME 类型。
fn mime_from_ext(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        Some("heic") | Some("heif") => "image/heic",
        Some("tiff") | Some("tif") => "image/tiff",
        _ => "image/png",
    }
}

/// 处理锁守卫：Drop 释放，panic / 提前 return 都不会遗留 BUSY=true。
struct BusyGuard;

impl Drop for BusyGuard {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::SeqCst);
    }
}

/// 获取处理锁（同时仅允许一个图片处理任务）。
fn acquire_busy() -> Result<BusyGuard, String> {
    if BUSY
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("已有图片处理任务进行中".into());
    }
    Ok(BusyGuard)
}

pub struct ImageExtension;

#[async_trait::async_trait]
impl Extension for ImageExtension {
    fn id(&self) -> &'static str {
        "image"
    }

    async fn setup(&self, _app: &AppHandle) -> tauri::Result<()> {
        Ok(())
    }
}
