use std::fs;
use std::path::{Path, PathBuf};

pub const SETTINGS_STORE_PATH: &str = "config/settings.json";

fn ensure_dir(path: &Path) {
    let _ = fs::create_dir_all(path);
}

pub fn icon_cache_dir() -> PathBuf {
    let dir = dirs::cache_dir()
        .unwrap_or_else(|| Path::new("/tmp").to_path_buf())
        .join("com.litiantao.voidnix")
        .join("icons");
    ensure_dir(&dir);
    dir
}
