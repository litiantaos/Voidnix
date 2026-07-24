use crate::runtime::registry::Extension;
use std::sync::atomic::{AtomicI32, Ordering};
use tauri::{AppHandle, Manager};

pub mod commands;
pub mod db;
#[cfg(target_os = "macos")]
pub mod monitor;

/// 剪贴板保留天数上限，由前端 config.ts 通过 invoke 推入。
/// 替代 Rust 直读 config.json（扩展自治 + 显式 invoke 推参，对齐 window-manager 样板）。
/// 默认 30 天（前端 config.ts 的 default 也是 30，启动时 immediate invoke 覆盖）。
static MAX_DAYS: AtomicI32 = AtomicI32::new(30);

/// Clipboard 扩展。
///
/// 拥有自己的 SQLite 数据库（`<app_data>/extensions/clipboard/clipboard.db`），
/// 通过 NSPasteboard 轮询监听剪贴板变化并落盘。
pub struct ClipboardExtension;

#[async_trait::async_trait]
impl Extension for ClipboardExtension {
    fn id(&self) -> &'static str {
        "clipboard"
    }

    async fn setup(&self, app: &AppHandle) -> tauri::Result<()> {
        // DB 打开 + DDL 全部 offload 到 worker 线程，避免阻塞 bootstrap join_all
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            // DB 打开失败不拖垮 app：不 manage、不启 monitor；命令侧 try_state 返回可读错误
            let path = match db::clipboard_db_path(&app) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("[clipboard] data dir unavailable, history disabled: {e}");
                    return;
                }
            };
            match db::Database::open(path) {
                Ok(database) => {
                    app.manage(database);
                    #[cfg(target_os = "macos")]
                    monitor::start_monitor(app.clone());
                }
                Err(e) => {
                    eprintln!("[clipboard] database open failed, history disabled: {e}");
                }
            }
        })
        .await
        .ok();
        Ok(())
    }
}

/// 由前端 config.ts watch(maxDays) + immediate invoke 推入。
#[tauri::command]
pub fn set_clipboard_max_days(max_days: i32) {
    MAX_DAYS.store(max_days, Ordering::Relaxed);
}

/// monitor 读取当前 MAX_DAYS（Relaxed：精确度不敏感，写入路径只用一次）。
pub(crate) fn load_max_days() -> i32 {
    MAX_DAYS.load(Ordering::Relaxed)
}
