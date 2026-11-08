//! 压缩时的 macOS / Windows 垃圾文件过滤（entry 级、纯函数）。

/// 目录级垃圾（整个子树剔除）。
const JUNK_DIRS: &[&str] = &[
    "__MACOSX",
    ".Spotlight-V100",
    ".fseventsd",
    ".Trashes",
    ".TemporaryItems",
];

/// 文件级垃圾（按文件名精确匹配或前缀匹配）。
const JUNK_FILE_EXACT: &[&str] = &[".DS_Store", "Thumbs.db", "desktop.ini"];
/// AppleDouble 资源分叉文件前缀（`._icon.png` 等）。
const APPLE_DOUBLE_PREFIX: &str = "._";

/// 判断归档内相对路径是否为垃圾条目（任一组件命中即剔除）。
pub fn is_junk(entry_path: &str) -> bool {
    entry_path.split('/').any(is_junk_component)
}

/// 隐藏条目（任一组件点开头且非 `.`/`..`：.git、.config、.DS_Store 等）。
/// 压缩时的「过滤隐藏文件」选项（独立于系统垃圾过滤，默认关）。
pub fn is_hidden(entry_path: &str) -> bool {
    entry_path.split('/').any(is_hidden_component)
}

fn is_hidden_component(component: &str) -> bool {
    component.starts_with('.') && component.len() > 1 && component != ".."
}

fn is_junk_component(component: &str) -> bool {
    if component.is_empty() || component == "." || component == ".." {
        return false;
    }
    JUNK_DIRS.contains(&component)
        || JUNK_FILE_EXACT.contains(&component)
        || (component.starts_with(APPLE_DOUBLE_PREFIX) && component.len() > 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_macos_junk() {
        assert!(is_junk(".DS_Store"));
        assert!(is_junk("a/b/.DS_Store"));
        assert!(is_junk("__MACOSX/foo/._bar"));
        assert!(is_junk("dir/._image.png"));
        assert!(is_junk("sd/.Trashes/501/x"));
        assert!(is_junk("Thumbs.db"));
    }

    #[test]
    fn keeps_normal_entries() {
        assert!(!is_junk("a/b/c.txt"));
        assert!(!is_junk("my.DS_Store.txt"));
        assert!(!is_junk("dir/__MACOSX_backup/x"));
        assert!(!is_junk("a..b"));
    }

    #[test]
    fn filters_hidden() {
        assert!(is_hidden(".git/objects/ab"));
        assert!(is_hidden("proj/.env"));
        assert!(is_hidden(".config"));
        assert!(!is_hidden("a/b/c.txt"));
        assert!(!is_hidden("."));
        assert!(!is_hidden("a/../b")); // `..` 非隐藏（路径清洗层另行拦截）
    }
}
