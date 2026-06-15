use crate::core::tier1::Tier1Extension;
use tauri::{AppHandle, Manager};

pub mod commands;
pub mod db;
#[cfg(target_os = "macos")]
pub mod monitor;

/// Clipboard 扩展。
///
/// 拥有自己的 SQLite 数据库（`<app_data>/extensions/clipboard/clipboard.db`），
/// 通过 NSPasteboard 轮询监听剪贴板变化并落盘。
pub struct Plugin;

impl Tier1Extension for Plugin {
    fn id(&self) -> &'static str {
        "clipboard"
    }

    fn on_setup(&self, app: &AppHandle) -> tauri::Result<()> {
        let db = db::Database::new(db::clipboard_db_path(app));
        app.manage(db);

        #[cfg(target_os = "macos")]
        monitor::start_monitor(app.clone());

        Ok(())
    }
}
