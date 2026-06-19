use std::path::Path;

/// 系统致命路径前缀（finder-ext 路径校验基线）。
const SYSTEM_CRITICAL: &[&str] = &["/System", "/usr/bin", "/bin", "/sbin"];

/// 验证路径安全性（finder-ext 单消费者）。
///
/// 拒绝：空路径、null 字节、无法 canonicalize 的路径、系统致命路径前缀。
/// canonicalize 同时解析符号链接——符号链接指向系统路径会被解析后拦截。
/// 允许 /Volumes、外接磁盘、网络挂载、/Library（用户主动操作应尊重）等。
///
/// 注：agent 命令执行有自己的命令级安全模型（native/policy.rs ExecPolicy），
///     不经此路径校验——路径是命令参数动态构造，无法预校验。
pub fn validate(path: &Path) -> bool {
    if path.as_os_str().is_empty() {
        return false;
    }
    if path.as_os_str().as_encoded_bytes().contains(&0) {
        return false;
    }
    let resolved = match path.canonicalize() {
        Ok(p) => p,
        Err(_) => return false,
    };
    let s = resolved.to_string_lossy();
    for prefix in SYSTEM_CRITICAL {
        if s.starts_with(prefix) {
            log::warn!("[path_guard] Rejected blocked path: {:?}", resolved);
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_blocked(prefix: &str) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let slug = prefix.trim_start_matches('/').replace('/', "_");
        let tmp = std::env::temp_dir().join(format!("voidnix-pg-{slug}-{id}"));
        let _ = std::fs::create_dir_all(&tmp);
        let link = tmp.join("escape_link");
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(prefix, &link).unwrap();
        // 符号链接指向系统路径：canonicalize 后应被拦
        let result = validate(&link);
        let _ = std::fs::remove_file(&link);
        let _ = std::fs::remove_dir(&tmp);
        assert!(!result, "expected {prefix:?} blocked via symlink");
    }

    #[test]
    fn rejects_empty_and_null() {
        assert!(!validate(Path::new("")));
        assert!(!validate(Path::new("\0")));
    }

    #[test]
    fn rejects_nonexistent() {
        assert!(!validate(Path::new("/definitely/not/here/voidnix")));
    }

    #[test]
    fn blocks_system_critical() {
        // 用符号链接指向系统路径，canonicalize 后命中前缀
        assert_blocked("/System");
        assert_blocked("/usr/bin");
        assert_blocked("/bin");
        assert_blocked("/sbin");
    }

    #[test]
    fn allows_library() {
        // 用户主动操作应尊重 /Library（不拦截）
        if Path::new("/Library").exists() {
            assert!(validate(Path::new("/Library")));
        }
    }

    #[test]
    fn allows_temp_dir() {
        let tmp = std::env::temp_dir();
        let test = tmp.join("voidnix-pg-ok");
        std::fs::write(&test, b"x").unwrap();
        assert!(validate(&test));
        let _ = std::fs::remove_file(&test);
    }
}
