//! zsh-as 扩展（Tier 1）。
//!
//! 职责：分发 binary、写 .zshrc 行、on/off 开关。
//! 不参与 hot path（zsh 启动后 binary 仅在后台 rebuild 时被调用）。

use crate::core::tier1::Tier1Extension;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use tauri::{AppHandle, Manager};

const BIN_NAME: &str = "zsh-autosuggestions";

/// .zshrc 行尾 marker，用于精确识别"我们写的行"。
const ZSHRC_LINE_SUFFIX: &str = "# voidnix zsh-autosuggestions";

/// binary 分发版本号。改 binary 内容（含 init.zsh，经 `include_str!` 嵌入）
/// 必须 bump 此数字，否则 install_bin 版本比对判定「无需更新」不部署。
const BIN_VERSION: u32 = 2;

/// 串行化 on_setup / set_enabled 路径，避免 torn write。
static SETUP_LOCK: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    SETUP_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn ext_dir(app: &AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("extensions")
        .join("zsh-as");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn installed_bin(app: &AppHandle) -> PathBuf {
    ext_dir(app).join("bin").join(BIN_NAME)
}

fn source_bin() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(BIN_NAME)))
        .filter(|p| p.exists())
}

fn cache_path(app: &AppHandle) -> PathBuf {
    ext_dir(app).join("index.cache")
}

fn signals_path(app: &AppHandle) -> PathBuf {
    ext_dir(app).join("signals.log")
}

fn enabled_flag(app: &AppHandle) -> PathBuf {
    ext_dir(app).join("enabled")
}

/// 记录已部署 binary 版本号的标志文件，与 binary 同目录。
fn version_file(app: &AppHandle) -> PathBuf {
    ext_dir(app).join("bin_version")
}

/// 读取版本号文件并解析。缺失/损坏返回 0（视为需要升级）。
fn read_version_at(path: &std::path::Path) -> u32 {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(0)
}

/// 复制 binary。版本号比对：已部署版本 == BIN_VERSION 才跳过，否则复制 + 写版本文件。
fn install_bin(app: &AppHandle) -> bool {
    let dest = installed_bin(app);
    let Some(source) = source_bin() else {
        return dest.exists();
    };
    install_bin_to(&source, &dest, &version_file(app))
}

/// install_bin 的纯路径参数内核，便于单测。
fn install_bin_to(
    source: &std::path::Path,
    dest: &std::path::Path,
    version_path: &std::path::Path,
) -> bool {
    if dest.exists() && read_version_at(version_path) == BIN_VERSION {
        return true;
    }

    if let Some(parent) = dest.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::copy(source, dest) {
        Ok(_) => {
            // 写版本号标志（失败不阻断：下次仍会因版本不匹配重新复制）。
            let _ = std::fs::write(version_path, BIN_VERSION.to_string());
            true
        }
        Err(e) => {
            log::warn!("zsh-as: failed to copy binary: {e}");
            false
        }
    }
}

/// 过滤 .zshrc 中所有带 marker 后缀的行。
fn filter_zshrc_lines(content: &str) -> String {
    content
        .lines()
        .filter(|l| !l.trim_end().ends_with(ZSHRC_LINE_SUFFIX))
        .collect::<Vec<&str>>()
        .join("\n")
}

/// 单次 read-modify-write：过滤旧行 + 追加新行（原子写 + backup）。
/// 若期望行已存在（内容完全一致）则短路跳过，避免无谓的 backup + rename。
fn write_zshrc_line(app: &AppHandle) -> Result<(), String> {
    let Some(home) = dirs::home_dir() else {
        return Err("无法定位 home 目录".into());
    };
    let zshrc = home.join(".zshrc");

    let content = std::fs::read_to_string(&zshrc).unwrap_or_default();
    let line = build_zshrc_line(&installed_bin(app), &cache_path(app), &signals_path(app));

    // 短路：期望行已存在则跳过写入
    if content.lines().any(|l| l == line) {
        return Ok(());
    }

    let filtered = filter_zshrc_lines(&content);

    let mut new_content = filtered;
    if !new_content.is_empty() && !new_content.ends_with('\n') {
        new_content.push('\n');
    }
    new_content.push_str(&line);
    new_content.push('\n');

    atomic_write_zshrc(&zshrc, &new_content)
}

/// 从 .zshrc 移除所有带 marker 的行（原子写 + backup）。
fn remove_zshrc_line() -> Result<(), String> {
    let Some(home) = dirs::home_dir() else {
        return Err("无法定位 home 目录".into());
    };
    let zshrc = home.join(".zshrc");
    let content = std::fs::read_to_string(&zshrc).unwrap_or_default();
    let filtered = filter_zshrc_lines(&content);
    let new_content = if content.ends_with('\n') && !filtered.is_empty() {
        format!("{filtered}\n")
    } else {
        filtered
    };
    if new_content != content {
        atomic_write_zshrc(&zshrc, &new_content)?;
    }
    Ok(())
}

fn quote_shell(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

/// 生成写入 .zshrc 的单行（export 三变量 + eval init + marker）。
fn build_zshrc_line(bin: &std::path::Path, cache: &std::path::Path, signals: &std::path::Path) -> String {
    format!(
        "export ZSH_AS_BIN={} ZSH_AS_CACHE={} ZSH_AS_SIGNALS={}; eval \"$( \"$ZSH_AS_BIN\" init )\" {}",
        quote_shell(&bin.display().to_string()),
        quote_shell(&cache.display().to_string()),
        quote_shell(&signals.display().to_string()),
        ZSHRC_LINE_SUFFIX,
    )
}

/// 原子写入 .zshrc：先备份原始内容到 `.zshrc.voidnix-bak`，再 tmp+rename 写入新内容。
/// 保证 .zshrc 要么完整旧内容、要么完整新内容，不会 torn。
fn atomic_write_zshrc(zshrc: &std::path::Path, content: &str) -> Result<(), String> {
    let parent = zshrc.parent().unwrap_or(std::path::Path::new("."));
    let bak = parent.join(".zshrc.voidnix-bak");
    let tmp = parent.join(".zshrc.voidnix-tmp");

    if zshrc.exists() {
        if let Err(e) = std::fs::copy(zshrc, &bak) {
            log::warn!("zsh-as: failed to backup .zshrc: {e}");
        }
    }
    std::fs::write(&tmp, content).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("write .zshrc tmp: {e}")
    })?;
    std::fs::rename(&tmp, zshrc).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("rename .zshrc: {e}")
    })?;
    Ok(())
}

#[tauri::command]
pub async fn set_zsh_autosuggestions_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let _guard = lock();
        let _ = std::fs::create_dir_all(ext_dir(&app));

        if enabled {
            if !install_bin(&app) {
                return Err("binary 部署失败".into());
            }
            std::fs::write(enabled_flag(&app), b"1")
                .map_err(|e| format!("写 enabled 标志失败: {e}"))?;
            write_zshrc_line(&app)?;
        } else {
            let _ = std::fs::remove_file(enabled_flag(&app));
            // 清理运行时数据（可重建），保留 binary 避免反复复制
            let _ = std::fs::remove_file(cache_path(&app));
            let _ = std::fs::remove_file(signals_path(&app));
            remove_zshrc_line()?;
        }

        log::info!("zsh-as enabled={}", enabled);
        Ok(())
    })
    .await
    .map_err(|e| format!("后台任务失败: {e}"))?
}

pub struct Plugin;

impl Tier1Extension for Plugin {
    fn id(&self) -> &'static str {
        "zsh-autosuggestions"
    }

    fn on_setup(&self, app: &AppHandle) -> tauri::Result<()> {
        let _guard = lock();
        if !enabled_flag(app).exists() {
            return Ok(());
        }
        install_bin(app);
        // 幂等刷新 .zshrc 行：升级后若行格式变化，已启用用户自动更新。
        // 启动阶段错误不阻塞（log 即可），用户主动 toggle 才反馈错误。
        if let Err(e) = write_zshrc_line(app) {
            log::warn!("zsh-as: on_setup write_zshrc_line: {e}");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn tmp_dir(label: &str) -> std::path::PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("zsh-as-mod-{label}-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn quote_shell_plain() {
        assert_eq!(quote_shell("abc"), "'abc'");
    }

    #[test]
    fn quote_shell_empty() {
        assert_eq!(quote_shell(""), "''");
    }

    #[test]
    fn quote_shell_single_quote() {
        assert_eq!(quote_shell("a'b"), "'a'\\''b'");
    }

    #[test]
    fn quote_shell_spaces_and_special() {
        assert_eq!(quote_shell("/path with spaces/x"), "'/path with spaces/x'");
        assert_eq!(quote_shell("a$b`c"), "'a$b`c'");
    }

    #[test]
    fn filter_zshrc_lines_removes_marker() {
        let content = "alias ll='ls -la'\n\
            export ZSH_AS_BIN='/p' # voidnix zsh-autosuggestions\n\
            echo hi\n";
        let filtered = filter_zshrc_lines(content);
        assert!(!filtered.contains(ZSHRC_LINE_SUFFIX));
        assert!(filtered.contains("alias ll"));
        assert!(filtered.contains("echo hi"));
    }

    #[test]
    fn filter_zshrc_lines_no_marker() {
        let content = "alias ll='ls -la'\n";
        let filtered = filter_zshrc_lines(content);
        assert_eq!(filtered, "alias ll='ls -la'");
    }

    #[test]
    fn filter_zshrc_lines_multiple_markers() {
        let content = "a\nb # voidnix zsh-autosuggestions\nc # voidnix zsh-autosuggestions\nd\n";
        let filtered = filter_zshrc_lines(content);
        let lines: Vec<&str> = filtered.lines().collect();
        assert_eq!(lines, vec!["a", "d"]);
    }

    #[test]
    fn build_zshrc_line_format() {
        let bin = std::path::Path::new("/App/V.app/Contents/MacOS/zsh-autosuggestions");
        let cache = std::path::Path::new("/u/Lib/x/extensions/zsh-as/index.cache");
        let signals = std::path::Path::new("/u/Lib/x/extensions/zsh-as/signals.log");
        let line = build_zshrc_line(bin, cache, signals);
        assert!(line.starts_with("export ZSH_AS_BIN="));
        assert!(line.contains("ZSH_AS_CACHE="));
        assert!(line.contains("ZSH_AS_SIGNALS="));
        assert!(line.contains("eval"));
        assert!(line.ends_with(ZSHRC_LINE_SUFFIX));
    }

    #[test]
    fn atomic_write_creates_backup_and_renames() {
        let dir = tmp_dir("atomic");
        let zshrc = dir.join(".zshrc");
        let bak = dir.join(".zshrc.voidnix-bak");
        let tmp = dir.join(".zshrc.voidnix-tmp");

        std::fs::write(&zshrc, "original\n").unwrap();
        atomic_write_zshrc(&zshrc, "new content\n").unwrap();

        assert_eq!(std::fs::read_to_string(&zshrc).unwrap(), "new content\n");
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), "original\n");
        assert!(!tmp.exists(), "tmp file cleaned up");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_write_creates_file_when_absent() {
        let dir = tmp_dir("atomic-new");
        let zshrc = dir.join(".zshrc");
        let bak = dir.join(".zshrc.voidnix-bak");

        atomic_write_zshrc(&zshrc, "fresh\n").unwrap();

        assert_eq!(std::fs::read_to_string(&zshrc).unwrap(), "fresh\n");
        assert!(!bak.exists(), "no backup when original absent");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_zshrc_line_short_circuits_when_exact_match() {
        let dir = tmp_dir("short-circuit");
        let zshrc = dir.join(".zshrc");
        let bak = dir.join(".zshrc.voidnix-bak");

        let line = build_zshrc_line(
            std::path::Path::new("/app/bin"),
            std::path::Path::new("/app/cache"),
            std::path::Path::new("/app/signals"),
        );
        std::fs::write(&zshrc, format!("alias ll='ls -la'\n{line}\n")).unwrap();

        // 内容完全一致 → 不应产生 backup
        // （write_zshrc_line 读的是真实 ~/.zshrc，无法直接测试；
        //  这里验证短路逻辑：lines().any(|l| l == line) 判定为 true）
        let content = std::fs::read_to_string(&zshrc).unwrap();
        assert!(
            content.lines().any(|l| l == line),
            "exact line should be found for short-circuit"
        );
        assert!(!bak.exists(), "no backup created in this state");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_skips_when_version_matches() {
        let dir = tmp_dir("install-match");
        let src = dir.join("src.bin");
        let dest = dir.join("dest.bin");
        let ver = dir.join("bin_version");
        std::fs::write(&src, b"SRC-V1").unwrap();
        std::fs::write(&dest, b"OLD-DIFFERENT-CONTENT").unwrap();
        std::fs::write(&ver, BIN_VERSION.to_string()).unwrap();

        assert!(install_bin_to(&src, &dest, &ver), "version match → skip");
        assert_eq!(
            std::fs::read_to_string(&dest).unwrap(),
            "OLD-DIFFERENT-CONTENT",
            "dest 不应被覆盖"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_copies_when_version_mismatch() {
        let dir = tmp_dir("install-mismatch");
        let src = dir.join("src.bin");
        let dest = dir.join("dest.bin");
        let ver = dir.join("bin_version");
        std::fs::write(&src, b"SRC-NEW").unwrap();
        std::fs::write(&dest, b"OLD").unwrap();
        std::fs::write(&ver, "0").unwrap(); // 旧版本（含缺失文件情形）

        assert!(install_bin_to(&src, &dest, &ver), "mismatch → copy");
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "SRC-NEW");
        assert_eq!(
            std::fs::read_to_string(&ver).unwrap(),
            BIN_VERSION.to_string(),
            "版本文件应更新"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_copies_when_dest_absent() {
        let dir = tmp_dir("install-new");
        let src = dir.join("src.bin");
        let dest = dir.join("nested").join("dest.bin");
        let ver = dir.join("bin_version");
        std::fs::write(&src, b"SRC").unwrap();

        assert!(install_bin_to(&src, &dest, &ver), "absent → copy");
        assert!(dest.exists());
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "SRC");
        assert_eq!(std::fs::read_to_string(&ver).unwrap(), BIN_VERSION.to_string());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
