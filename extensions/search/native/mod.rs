mod types;

mod app_discovery;
mod cache;
pub mod commands;
mod icon;

pub use cache::{init_app_watcher, prewarm_cache, set_app_handle};

use crate::runtime::registry::Extension;

/// 命令注册（局部 invoke_handler）。
pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new("search").build()
}

/// Search 扩展。统一走 registry bootstrap（不再绕过）。
pub struct SearchExtension;

#[async_trait::async_trait]
impl Extension for SearchExtension {
    fn id(&self) -> &'static str {
        "search"
    }

    async fn setup(&self, app: &tauri::AppHandle) -> tauri::Result<()> {
        set_app_handle(app.clone());
        init_app_watcher();
        tauri::async_runtime::spawn(prewarm_cache());
        Ok(())
    }
}
