//! 归档扩展：zip/tar.gz 压缩（垃圾过滤）+ 多格式解压 + 包内预览。

mod bsdtar;
mod compress;
mod extract;
mod format;
mod inspect;
mod job;
mod junk;
mod preview;
mod rar;
mod sevenz;

use crate::runtime::registry::Extension;
use std::path::Path;
use tauri::ipc::Channel;
use tauri::AppHandle;

pub use extract::{ExtractRequest, ExtractResult};
pub use inspect::ArchiveInfo;
pub use job::ArchiveEvent;
pub use preview::EntryPreview;

/// 列出归档内容。
#[tauri::command]
pub async fn archive_inspect(path: String) -> Result<ArchiveInfo, String> {
    tokio::task::spawn_blocking(move || {
        let p = Path::new(&path);
        if !crate::platform::path_guard::validate(p) {
            return Err("路径不可用".into());
        }
        let f = format::detect(p).ok_or("无法识别的归档格式")?;
        inspect::inspect(p, f)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 包内单条目预览（文本/图片；加密 zip 需带密码）。
#[tauri::command]
pub async fn archive_preview(
    path: String,
    entry: String,
    password: Option<String>,
) -> Result<EntryPreview, String> {
    tokio::task::spawn_blocking(move || {
        let p = Path::new(&path);
        if !crate::platform::path_guard::validate(p) {
            return Err("路径不可用".into());
        }
        let f = format::detect(p).ok_or("无法识别的归档格式")?;
        preview::preview(p, f, &entry, password.as_deref())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 解压（全量或选中条目）；进度经 Channel 推送。
#[tauri::command]
pub async fn archive_extract(
    request: ExtractRequest,
    on_event: Channel<ArchiveEvent>,
) -> Result<ExtractResult, String> {
    let token = match job::begin() {
        Ok(t) => t,
        Err(e) => {
            // 任务槽被占用：经事件通道给出明确文案（invoke 同时 reject）
            job::emit(&on_event, ArchiveEvent::Error { message: e.clone() });
            return Err(e);
        }
    };
    let res = tokio::task::spawn_blocking(move || {
        let r = extract::extract_blocking(&on_event, &request, &token);
        match &r {
            Ok(result) => {
                job::emit(
                    &on_event,
                    ArchiveEvent::Done {
                        output_path: result.output_dir.clone(),
                        file_count: result.file_count,
                    },
                );
            }
            Err(msg) => {
                job::emit(
                    &on_event,
                    ArchiveEvent::Error {
                        message: msg.clone(),
                    },
                );
            }
        }
        r
    })
    .await;
    // 线程 panic（JoinError）同样释放任务槽，否则 BUSY 永久占住
    job::end();
    res.map_err(|e| e.to_string())?
}

/// 压缩（zip/tar.gz + 垃圾过滤）；进度经 Channel 推送。
#[tauri::command]
pub async fn archive_compress(
    request: compress::CompressRequest,
    on_event: Channel<ArchiveEvent>,
) -> Result<compress::CompressResult, String> {
    let token = match job::begin() {
        Ok(t) => t,
        Err(e) => {
            job::emit(&on_event, ArchiveEvent::Error { message: e.clone() });
            return Err(e);
        }
    };
    let res = tokio::task::spawn_blocking(move || {
        let r = compress::compress_blocking(&on_event, &request, &token);
        match &r {
            Ok(result) => {
                job::emit(
                    &on_event,
                    ArchiveEvent::Done {
                        output_path: result.output_path.clone(),
                        file_count: result.file_count,
                    },
                );
            }
            Err(msg) => {
                job::emit(
                    &on_event,
                    ArchiveEvent::Error {
                        message: msg.clone(),
                    },
                );
            }
        }
        r
    })
    .await;
    job::end();
    res.map_err(|e| e.to_string())?
}

/// 取消进行中的任务。
#[tauri::command]
pub async fn archive_cancel() -> Result<bool, String> {
    Ok(job::cancel())
}

pub struct ArchiveExtension;

#[async_trait::async_trait]
impl Extension for ArchiveExtension {
    fn id(&self) -> &'static str {
        "archive"
    }

    async fn setup(&self, _app: &AppHandle) -> tauri::Result<()> {
        Ok(())
    }
}
