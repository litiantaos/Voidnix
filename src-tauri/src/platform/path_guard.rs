use std::path::Path;

/// 路径校验信任级（§2.6）。
///
/// 不同调用方信任级不同：finder-ext 是用户主动右键操作（尊重用户选择），
/// agent 是 AI 自动执行（严格黑名单 + canonicalize 防符号链接逃逸）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    /// finder-ext：用户主动操作，仅拦系统致命路径。
    Interactive,
    /// agent：AI 自动执行，在 Interactive 基础上加严 + canonicalize 防符号链接逃逸。
    #[allow(dead_code)]
    Automated,
}

/// 系统致命路径前缀（Interactive 基线，两类 policy 共同拦截）。
const SYSTEM_CRITICAL: &[&str] = &["/System", "/usr/bin", "/bin", "/sbin"];

/// Automated 额外拦截（系统库 / 包管理器根，防 AI 自动改系统级文件）。
const AUTOMATED_EXTRA: &[&str] = &["/Library", "/opt/homebrew"];

/// 验证路径安全性。
///
/// 两级均：拒绝空路径、null 字节、无法 canonicalize 的路径（canonicalize
/// 同时解析符号链接——Automated 防符号链接逃逸依赖此）。
///
/// - `Interactive`：拦 `SYSTEM_CRITICAL`
/// - `Automated`：拦 `SYSTEM_CRITICAL` ∪ `AUTOMATED_EXTRA`
///
/// 允许 /Volumes、外接磁盘、网络挂载等。
pub fn validate(path: &Path, policy: Policy) -> bool {
    if path.as_os_str().is_empty() {
        return false;
    }
    if path.as_os_str().as_encoded_bytes().contains(&0) {
        return false;
    }
    // canonicalize 解析符号链接：Automated 下若符号链接指向系统路径，会被前缀拦截。
    let resolved = match path.canonicalize() {
        Ok(p) => p,
        Err(_) => return false,
    };
    let s = resolved.to_string_lossy();

    let blocked: &[&[&str]] = match policy {
        Policy::Interactive => &[SYSTEM_CRITICAL],
        Policy::Automated => &[SYSTEM_CRITICAL, AUTOMATED_EXTRA],
    };

    for group in blocked {
        for prefix in *group {
            if s.starts_with(prefix) {
                log::warn!("[path_guard] Rejected blocked path: {:?}", resolved);
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_blocked(policy: Policy, prefix: &str) {
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
        let result = validate(&link, policy);
        let _ = std::fs::remove_file(&link);
        let _ = std::fs::remove_dir(&tmp);
        assert!(!result, "expected {prefix:?} blocked under {policy:?} via symlink");
    }

    #[test]
    fn rejects_empty_and_null() {
        assert!(!validate(Path::new(""), Policy::Interactive));
        assert!(!validate(Path::new("\0"), Policy::Interactive));
    }

    #[test]
    fn rejects_nonexistent() {
        assert!(!validate(Path::new("/definitely/not/here/voidnix"), Policy::Interactive));
    }

    #[test]
    fn interactive_blocks_system_critical() {
        // 用符号链接指向系统路径，canonicalize 后命中前缀
        assert_blocked(Policy::Interactive, "/System");
        assert_blocked(Policy::Interactive, "/usr/bin");
        assert_blocked(Policy::Interactive, "/bin");
        assert_blocked(Policy::Interactive, "/sbin");
    }

    #[test]
    fn automated_blocks_library_and_homebrew() {
        assert_blocked(Policy::Automated, "/Library");
        assert_blocked(Policy::Automated, "/opt/homebrew");
    }

    #[test]
    fn interactive_allows_library() {
        // Interactive 不拦 /Library（用户主动操作应尊重）
        // 用真实存在的 /Library 路径
        if Path::new("/Library").exists() {
            assert!(validate(Path::new("/Library"), Policy::Interactive));
            assert!(!validate(Path::new("/Library"), Policy::Automated));
        }
    }

    #[test]
    fn allows_temp_dir() {
        let tmp = std::env::temp_dir();
        let test = tmp.join("voidnix-pg-ok");
        std::fs::write(&test, b"x").unwrap();
        assert!(validate(&test, Policy::Interactive));
        assert!(validate(&test, Policy::Automated));
        let _ = std::fs::remove_file(&test);
    }
}
