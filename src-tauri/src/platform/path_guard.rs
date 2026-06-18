use std::path::Path;

/// 禁止访问的系统路径前缀。
const BLOCKED_PREFIXES: &[&str] = &[
    "/System",
    "/private/etc",
    "/private/var/db",
    "/usr/bin",
    "/usr/sbin",
    "/usr/lib",
    "/usr/libexec",
    "/bin",
    "/sbin",
    "/Library",
    "/opt/homebrew",
];

/// 验证路径安全性：拒绝空路径、null 字节、不存在的路径、系统关键路径。
///
/// 允许 /Volumes、外接磁盘、网络挂载等。
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
    for prefix in BLOCKED_PREFIXES {
        if s.starts_with(prefix) {
            log::warn!("[path_guard] Rejected blocked path: {:?}", resolved);
            return false;
        }
    }
    true
}
