//! 文件搜索目录白名单：受 TCC 管辖的目录仅在全磁盘权限就绪后纳入扫描与 usage 查询。
//!
//! `FILE_SCAN_DIRS` 恒定扫描；`FILE_SCAN_TCC_DIRS` 首次触碰（read_dir / FSEvents）
//! 会触发系统「访问文件夹」询问，因此按 `check_full_disk_access()` 门控。
//! 文件索引与 usage 索引必须使用同一份目录集合：否则索引里有文件、usage 却为零，
//! 常用项无法前置。

/// 不受「文件与文件夹」权限管辖、恒定扫描的目录。
pub(super) const FILE_SCAN_DIRS: &[&str] = &["Projects", "Code"];

/// 受 TCC「文件与文件夹」管辖、仅全磁盘权限就绪后扫描的目录。
pub(super) const FILE_SCAN_TCC_DIRS: &[&str] = &[
    "Desktop",
    "Documents",
    "Downloads",
    "Pictures",
    "Music",
    "Movies",
];

/// 本进程当前可扫描的目录名集合。
pub(super) fn scan_dir_names(tcc_allowed: bool) -> impl Iterator<Item = &'static str> {
    FILE_SCAN_DIRS.iter().copied().chain(
        FILE_SCAN_TCC_DIRS
            .iter()
            .copied()
            .filter(move |_| tcc_allowed),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// usage 查询与索引扫描共用 scan_dir_names：TCC 未授权时只查 Projects/Code，
    /// 授权后 Documents/Downloads 等也进入 use_count 查询（常用文件排序的前提）。
    #[test]
    fn scan_dirs_include_tcc_only_when_allowed() {
        let restricted: Vec<_> = scan_dir_names(false).collect();
        assert_eq!(restricted, ["Projects", "Code"]);

        let allowed: Vec<_> = scan_dir_names(true).collect();
        assert!(allowed.contains(&"Documents"));
        assert!(allowed.contains(&"Downloads"));
    }
}
