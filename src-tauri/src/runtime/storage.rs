// 存储相关常量与工具。
// 扩展配置路径约定：extensions/<id>/config.json（由前端 defineConfig 管理）。
// TempHandle：扩展注册临时文件，框架统一清理。

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// 临时文件注册表（全局单例）。
/// 扩展注册临时文件路径，框架退出时统一清理。
static TEMP_FILES: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// 注册一个临时文件路径，框架退出时自动清理。
pub fn register_temp(path: PathBuf) {
    if let Ok(mut files) = TEMP_FILES.lock() {
        if !files.contains(&path) {
            files.push(path);
        }
    }
}

/// 注销一个临时文件（扩展自行删除后调用）。
pub fn unregister_temp(path: &Path) {
    if let Ok(mut files) = TEMP_FILES.lock() {
        files.retain(|p| p != path);
    }
}

/// 清理所有已注册的临时文件。框架 teardown 或定期维护时调用。
pub fn cleanup_all_temps() {
    if let Ok(mut files) = TEMP_FILES.lock() {
        for path in files.drain(..) {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// 清理指定前缀的临时文件（兼容旧版 screenshot 直接扫 /tmp 的方式）。
pub fn cleanup_temps_by_prefix(tmp_dir: &Path, prefix: &str, extensions: &[&str]) {
    if let Ok(entries) = std::fs::read_dir(tmp_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(prefix) && extensions.iter().any(|ext| name.ends_with(ext)) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}
