mod types;

mod app_discovery;
mod cache;
pub mod commands;
mod icon;
mod pinyin;

pub use cache::{init_fs_watchers, prewarm_cache, set_app_handle};

use crate::runtime::registry::Extension;

/// Search 扩展。统一走 registry bootstrap（不再绕过）。
pub struct SearchExtension;

#[async_trait::async_trait]
impl Extension for SearchExtension {
    fn id(&self) -> &'static str {
        "search"
    }

    async fn setup(&self, app: &tauri::AppHandle) -> tauri::Result<()> {
        set_app_handle(app.clone());
        init_fs_watchers();
        tauri::async_runtime::spawn(prewarm_cache());
        Ok(())
    }
}
