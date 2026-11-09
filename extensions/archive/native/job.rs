//! 任务槽：压缩/解压共用单飞（同一时刻仅一个任务）+ 取消令牌 + Channel 事件投递。

use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::ipc::Channel;
use tokio_util::sync::CancellationToken;

static BUSY: AtomicBool = AtomicBool::new(false);
static CANCEL: Mutex<Option<CancellationToken>> = Mutex::new(None);

/// 任务事件（经本次 invoke 的 Channel 投递；progress 仅在按钮文案内呈现百分比）。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ArchiveEvent {
    #[serde(rename = "started")]
    Started { total_bytes: u64 },
    #[serde(rename = "progress")]
    Progress { percent: f64 },
    #[serde(rename = "done")]
    Done {
        output_path: String,
        file_count: u64,
    },
    #[serde(rename = "error")]
    Error { message: String },
}

/// 占用任务槽；已有任务进行中则拒绝。
pub fn begin() -> Result<CancellationToken, String> {
    if BUSY.swap(true, Ordering::SeqCst) {
        return Err("已有压缩/解压任务进行中".into());
    }
    let token = CancellationToken::new();
    *CANCEL.lock().unwrap() = Some(token.clone());
    Ok(token)
}

/// 释放任务槽（任务终态后调用；panic 安全性由命令层 join 错误路径兜底）。
pub fn end() {
    *CANCEL.lock().unwrap() = None;
    BUSY.store(false, Ordering::SeqCst);
}

/// 取消进行中的任务（无任务返回 false）。
pub fn cancel() -> bool {
    let guard = CANCEL.lock().unwrap();
    match guard.as_ref() {
        Some(t) => {
            t.cancel();
            true
        }
        None => false,
    }
}

/// 经本次 invoke 的 Channel 投递。
pub fn emit(on_event: &Channel<ArchiveEvent>, ev: ArchiveEvent) {
    let _ = on_event.send(ev);
}
