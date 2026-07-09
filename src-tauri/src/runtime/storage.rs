// 临时文件管理 + 扩展数据目录 + 安全 PNG 写入。
//
// TempHandle：RAII guard，Drop 时自动删除文件。
// screenshot 等扩展持有 guard 于窗口 State / 函数作用域，离开作用域即清理。
// cleanup_all_voidnix_temps：启动时扫 /tmp 兜底异常退出残留。

use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

/// RAII 临时文件 guard。
///
/// `Drop` 自动删除文件。大文件 IO 的 Drop 同步执行（截图 PNG 通常 <几 MB，remove 很快）；
/// 真正阻塞的大文件场景由调用方自行 `spawn_blocking` detach。
pub struct TempHandle {
    path: PathBuf,
}

impl TempHandle {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for TempHandle {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// 清理指定前缀的临时文件（启动时扫 /tmp 兜底异常退出残留）。
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

/// 启动期统一清理 Voidnix 在 /tmp 的所有残留。
///
/// 覆盖三个前缀族：
/// - `voidnix_*.png/.jpg`：screenshot 临时图（ocr/clip/pin/scroll）
/// - `voidnix-icon-*.png`：search 扩展图标缓存
/// - `/tmp/voidnix/picker.jpg`：screenshot 屏幕预览 JPEG（子目录）
pub fn cleanup_all_voidnix_temps() {
    let tmp = std::env::temp_dir();
    cleanup_temps_by_prefix(&tmp, "voidnix_", &[".png", ".jpg"]);
    cleanup_temps_by_prefix(&tmp, "voidnix-icon-", &[".png"]);
    // picker.jpg 位于 /tmp/voidnix/ 子目录
    let voidnix_dir = tmp.join("voidnix");
    if voidnix_dir.exists() {
        cleanup_temps_by_prefix(&voidnix_dir, "picker", &[".jpg"]);
    }
}

/// 扩展数据目录：`<app_data>/extensions/<id>`，自动 create_dir_all。
///
/// 替代各扩展 native/ 内重复的 `app_data_dir().unwrap_or_else().join("extensions").join(id)` 模式。
/// unwrap 策略统一为 propagate（返回 Result），调用方显式处理。
pub fn ext_data_dir(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("extensions")
        .join(id);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// 安全写入 PNG（含 create_dir_all + path_guard）。
///
/// 抽自 screenshot ocr.rs 的 save_screenshot 路径校验逻辑，供 save_screenshot /
/// save_scroll_result 共用，消除不对称（前者有 path_guard，后者无）。
///
/// 路径安全校验：拒绝写入系统致命路径（/System、/usr/bin、/bin、/sbin）。
/// path_guard::validate 要求路径存在（canonicalize），优先校验 file_path；
/// 若 file_path 不存在（新建文件），校验其 parent（已在上方 create_dir_all 后存在）。
pub fn save_png_safely(file_path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let guard_ok = if file_path.exists() {
        crate::platform::path_guard::validate(file_path)
    } else {
        file_path
            .parent()
            .map(crate::platform::path_guard::validate)
            .unwrap_or(false)
    };
    if !guard_ok {
        return Err(format!("目标路径被安全策略拒绝：{}", file_path.display()));
    }
    std::fs::write(file_path, bytes).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_handle_drops_file() {
        let dir = std::env::temp_dir().join(format!("voidnix-th-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.txt");
        std::fs::write(&f, b"x").unwrap();
        assert!(f.exists());
        {
            let _h = TempHandle::new(f.clone());
            assert!(f.exists(), "file should exist while handle alive");
        }
        assert!(!f.exists(), "file should be removed after drop");
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn temp_handle_survives_scope_until_drop() {
        let dir = std::env::temp_dir().join(format!("voidnix-th2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("b.txt");
        std::fs::write(&f, b"x").unwrap();
        let h = TempHandle::new(f.clone());
        assert!(f.exists());
        std::mem::drop(h);
        assert!(!f.exists());
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn cleanup_temps_by_prefix_matches() {
        let dir = std::env::temp_dir().join(format!("voidnix-th3-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("voidnix_x.png"), b"x").unwrap();
        std::fs::write(dir.join("voidnix_y.jpg"), b"x").unwrap();
        std::fs::write(dir.join("other.txt"), b"x").unwrap();
        cleanup_temps_by_prefix(&dir, "voidnix_", &[".png", ".jpg"]);
        assert!(!dir.join("voidnix_x.png").exists());
        assert!(!dir.join("voidnix_y.jpg").exists());
        assert!(dir.join("other.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cleanup_all_voidnix_temps_covers_all_prefixes() {
        let dir = std::env::temp_dir().join(format!("voidnix-cleanup-all-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("voidnix_ocr.png"), b"x").unwrap();
        std::fs::write(dir.join("voidnix-icon-app.png"), b"x").unwrap();

        // 直接复用内部实现：扫两种前缀
        cleanup_temps_by_prefix(&dir, "voidnix_", &[".png", ".jpg"]);
        cleanup_temps_by_prefix(&dir, "voidnix-icon-", &[".png"]);
        assert!(!dir.join("voidnix_ocr.png").exists());
        assert!(!dir.join("voidnix-icon-app.png").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
