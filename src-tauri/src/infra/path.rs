use std::fs;
use std::path::{Path, PathBuf};
use tauri::Manager;

pub const SETTINGS_STORE_PATH: &str = "config/settings.json";

fn ensure_dir(path: &Path) {
    let _ = fs::create_dir_all(path);
}

fn app_base(app: &tauri::AppHandle) -> PathBuf {
    app.path().app_data_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn clipboard_db_path(app: &tauri::AppHandle) -> PathBuf {
    let dir = app_base(app).join("data");
    ensure_dir(&dir);
    dir.join("clipboard.db")
}

pub fn finder_ext_command_dir(app: &tauri::AppHandle) -> PathBuf {
    let dir = app_base(app).join("extensions").join("finder-ext").join("commands");
    ensure_dir(&dir);
    dir
}

pub fn zsh_daemon_dir(app: &tauri::AppHandle) -> PathBuf {
    let dir = app_base(app).join("extensions").join("zsh-autosuggestions");
    ensure_dir(&dir);
    dir
}

pub fn zsh_daemon_bin_path(app: &tauri::AppHandle) -> PathBuf {
    let dir = zsh_daemon_dir(app).join("bin");
    ensure_dir(&dir);
    dir.join("zsh-autosuggestions")
}

pub fn zsh_daemon_flag_path(app: &tauri::AppHandle) -> PathBuf {
    zsh_daemon_dir(app).join("enabled")
}

pub fn icon_cache_dir() -> PathBuf {
    let dir = dirs::cache_dir()
        .unwrap_or_else(|| Path::new("/tmp").to_path_buf())
        .join("com.litiantao.voidnix")
        .join("icons");
    ensure_dir(&dir);
    dir
}
