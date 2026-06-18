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
        .join("extensions")
        .join("search")
        .join("icons");
    ensure_dir(&dir);
    dir
}

/// 清理 icon 缓存：先按年龄删除超 `max_age_days` 天未修改的文件，再按数量淘汰超过
/// `max_count` 个的最旧文件。启动时调用一次，避免缓存无限增长。
#[allow(dead_code)]
pub fn cleanup_icon_cache(max_count: usize, max_age_days: u64) {
    let dir = icon_cache_dir();
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let max_age_secs = max_age_days.saturating_mul(24 * 60 * 60);

    let mut files: Vec<(PathBuf, u64)> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let mtime = e
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())?;
            Some((path, mtime))
        })
        .collect();

    let mut removed = 0usize;

    // 1. 按年龄删除：超 max_age_days 未修改
    files.retain(|(path, mtime)| {
        if now.saturating_sub(*mtime) > max_age_secs {
            let _ = fs::remove_file(path);
            removed += 1;
            false
        } else {
            true
        }
    });

    // 2. 按数量删除：超过 max_count，删最旧的
    if files.len() > max_count {
        files.sort_by_key(|(_, mtime)| *mtime);
        let drop_count = files.len().saturating_sub(max_count);
        for (path, _) in files.iter().take(drop_count) {
            let _ = fs::remove_file(path);
        }
        removed += drop_count;
    }

    if removed > 0 {
        log::info!("[icon] cache cleanup: removed {} files", removed);
    }
}
