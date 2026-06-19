// 临时文件管理（§2.7）。
//
// TempHandle：RAII guard，Drop 时自动删除文件。
// screenshot 等扩展持有 guard 于窗口 State / 函数作用域，离开作用域即清理。
// cleanup_temps_by_prefix：启动时扫 /tmp 兜底异常退出残留。

use std::path::{Path, PathBuf};

/// RAII 临时文件 guard（§2.7）。
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
}
