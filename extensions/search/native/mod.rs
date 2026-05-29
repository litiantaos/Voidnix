mod types;
#[allow(unused_imports)]
pub use types::SearchResult;

mod pinyin;
mod icon;
mod app_discovery;
mod cache;
pub mod commands;

pub use cache::{init_app_watcher, prewarm_cache, set_app_handle};

pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new("search")
        .setup(|app, _api| {
            crate::extensions::search::set_app_handle(app.clone());
            crate::extensions::search::init_app_watcher();
            tauri::async_runtime::spawn(crate::extensions::search::prewarm_cache());
            Ok(())
        })
        .build()
}
