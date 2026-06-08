use super::ext_loader::ExtensionLoader;
use super::ext_manifest::{LoadedExtension, Manifest};
use tauri::{AppHandle, State};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// 扩展加载器状态（由 app.manage() 持有）
pub struct ExtensionLoaderState(Mutex<ExtensionLoader>);

impl ExtensionLoaderState {
    pub fn new() -> Self {
        Self(Mutex::new(ExtensionLoader::new()))
    }

    pub(crate) fn loader(&self) -> std::sync::MutexGuard<'_, ExtensionLoader> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Default for ExtensionLoaderState {
    fn default() -> Self {
        Self::new()
    }
}

/// 列出所有已加载的 Tier 2 扩展
#[tauri::command]
pub fn ext_list(state: State<'_, ExtensionLoaderState>) -> Vec<Manifest> {
    state
        .loader()
        .list()
        .into_iter()
        .map(|ext: Arc<LoadedExtension>| ext.manifest.clone())
        .collect()
}

/// 安装 .vnext 包
#[tauri::command]
pub async fn ext_install(
    app: AppHandle,
    state: State<'_, ExtensionLoaderState>,
    vnext_path: String,
) -> Result<String, String> {
    let path = PathBuf::from(vnext_path);
    state.loader().install(&app, &path)
}

/// 卸载扩展
#[tauri::command]
pub fn ext_uninstall(
    app: AppHandle,
    state: State<'_, ExtensionLoaderState>,
    id: String,
) -> Result<(), String> {
    state.loader().uninstall(&app, &id)
}

/// 获取扩展入口文件内容
#[tauri::command]
pub fn ext_entry_content(state: State<'_, ExtensionLoaderState>, id: String) -> Option<String> {
    state.loader().entry_content(&id)
}

/// 获取扩展 README 内容
#[tauri::command]
pub fn ext_readme(state: State<'_, ExtensionLoaderState>, id: String) -> Option<String> {
    state.loader().readme_content(&id)
}

/// 重新扫描扩展目录
#[tauri::command]
pub fn ext_rescan(app: AppHandle, state: State<'_, ExtensionLoaderState>) -> Result<Vec<String>, String> {
    state.loader().rescan(&app)
}
