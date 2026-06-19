use crate::runtime::registry::Extension;
use tauri::{AppHandle, Manager};

pub mod commands;
pub mod db;
#[cfg(target_os = "macos")]
pub mod monitor;

/// 命令注册（局部 invoke_handler，§2.8）。
pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new("clipboard")
        .build()
}

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
        let db = db::Database::new(db::clipboard_db_path(app));
        app.manage(db);

        #[cfg(target_os = "macos")]
        monitor::start_monitor(app.clone());

        Ok(())
    }
}
