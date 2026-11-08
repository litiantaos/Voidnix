//! 包内单条目预览：文本 / 图片 base64 / 二进制元信息（PDF 等不做，无 QL 面板）。

use base64::Engine;
use serde::Serialize;

use super::format::ArchiveFormat;
use super::inspect::read_entry;
use std::path::Path;

/// 预览内容字节上限（2MB；超过截断并标记）。
pub const PREVIEW_CAP: usize = 2 * 1024 * 1024;
/// 二进制嗅探窗口（前 8KB 含 NUL 即判二进制）。
const SNIFF_WINDOW: usize = 8 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryPreview {
    pub payload: PreviewPayload,
    /// 超过 PREVIEW_CAP 截断（文本截断展示 / 图片直接判 Binary）。
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PreviewPayload {
    Text { content: String },
    Image { mime: String, base64: String },
    Binary,
}

pub fn preview(
    path: &Path,
    format: ArchiveFormat,
    entry: &str,
    password: Option<&str>,
) -> Result<EntryPreview, String> {
    let (bytes, truncated) = read_entry(path, format, entry, password)?;
    if let Some(mime) = image_mime(entry) {
        // 图片超限不再部分渲染（base64 膨胀 1.33x，直接归二进制）
        if !truncated && bytes.len() <= PREVIEW_CAP {
            return Ok(EntryPreview {
                payload: PreviewPayload::Image {
                    mime: mime.to_string(),
                    base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
                },
                truncated: false,
            });
        }
        return Ok(EntryPreview {
            payload: PreviewPayload::Binary,
            truncated,
        });
    }
    let window = &bytes[..bytes.len().min(SNIFF_WINDOW)];
    if window.contains(&0) {
        return Ok(EntryPreview {
            payload: PreviewPayload::Binary,
            truncated,
        });
    }
    Ok(EntryPreview {
        payload: PreviewPayload::Text {
            content: String::from_utf8_lossy(&bytes).into_owned(),
        },
        truncated,
    })
}

/// 按条目扩展名识别可内联渲染的图片格式。
fn image_mime(entry: &str) -> Option<&'static str> {
    let ext = entry.rsplit('.').next()?.to_ascii_lowercase();
    match ext.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "bmp" => Some("image/bmp"),
        "svg" => Some("image/svg+xml"),
        _ => None,
    }
}
