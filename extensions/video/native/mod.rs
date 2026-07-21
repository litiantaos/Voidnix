//! 视频处理扩展：压缩 / 格式转换 / 提取音频（FFmpeg）。

mod args;
mod core;
mod job;

use crate::runtime::registry::Extension;
use tauri::ipc::Channel;
use tauri::AppHandle;

pub use core::{CoreStatus, VideoMeta};
pub use job::{JobSnapshot, RunRequest, VideoEvent};

/// 查询 FFmpeg 内核状态。
#[tauri::command]
pub async fn video_core_status(app: AppHandle) -> Result<CoreStatus, String> {
    // 同步 Command（ffmpeg -version）放 blocking，避免卡 runtime
    tokio::task::spawn_blocking(move || core::core_status(&app))
        .await
        .map_err(|e| e.to_string())
}

/// 确保 FFmpeg 就绪（系统 PATH 或按需下载）。
#[tauri::command]
pub async fn video_ensure_core(app: AppHandle) -> Result<CoreStatus, String> {
    core::ensure_bins(&app).await?;
    tokio::task::spawn_blocking(move || core::core_status(&app))
        .await
        .map_err(|e| e.to_string())
}

/// 探测视频元数据。
#[tauri::command]
pub async fn video_probe(app: AppHandle, path: String) -> Result<VideoMeta, String> {
    // 同步 Command 放 blocking 线程，避免卡 runtime
    tokio::task::spawn_blocking(move || core::probe(&app, &path))
        .await
        .map_err(|e| e.to_string())?
}

/// 启动转码；进度通过 Channel 推送。
#[tauri::command]
pub async fn video_run(
    app: AppHandle,
    request: RunRequest,
    on_event: Channel<VideoEvent>,
) -> Result<(), String> {
    job::run_job(app, request, on_event).await
}

/// 取消当前转码。
#[tauri::command]
pub async fn video_cancel() -> Result<bool, String> {
    Ok(job::cancel_job())
}

/// 查询任务状态（重开面板恢复 UI）。
#[tauri::command]
pub async fn video_job_status() -> Result<JobSnapshot, String> {
    Ok(job::job_status())
}

pub struct VideoExtension;

#[async_trait::async_trait]
impl Extension for VideoExtension {
    fn id(&self) -> &'static str {
        "video"
    }

    async fn setup(&self, _app: &AppHandle) -> tauri::Result<()> {
        Ok(())
    }
}
