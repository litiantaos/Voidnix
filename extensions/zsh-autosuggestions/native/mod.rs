//! zsh-as 扩展（Tier 1）。
//!
//! 职责：分发 binary、写 .zshrc 行、on/off 开关。
//! 不参与 hot path（zsh 启动后 binary 仅在后台 rebuild 时被调用）。

use crate::runtime::registry::Extension;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use tauri::AppHandle;

const BIN_NAME: &str = "zsh-autosuggestions";

/// .zshrc scope id（marker `# voidnix zsh-autosuggestions`，见 runtime/shell_rc）。
const ZSHRC_SCOPE: &str = "zsh-autosuggestions";

/// binary 分发版本号。install_bin 比对此常量与 `bin_version` 文件，相等即跳过复制。
/// **每改 binary 内容（`native/src/*.rs` 或 `include_str!` 嵌入的 `init.zsh`）
/// 必须 bump**——开发期迭代亦然，共用版本号会导致改动不部署。
const BIN_VERSION: u32 = 8;

/// 串行化 setup / set_enabled 路径，避免 torn write。
static SETUP_LOCK: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    SETUP_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn ext_dir(app: &AppHandle) -> PathBuf {
    crate::runtime::storage::ext_data_dir(app, "zsh-autosuggestions")
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn installed_bin(app: &AppHandle) -> PathBuf {
    ext_dir(app).join("bin").join(BIN_NAME)
}

/// binary 非空判定：存在且大小 > 0（不校验可执行位，由 install_bin_to 复制后 set_mode 兜底）。
/// 防御 cargo build 失败/中断留下的 0 字节占位文件——若不校验，
/// 复制空 binary 到 ext dir 后 zsh `eval "$($BIN init)"` 输出为空，
/// 补全静默失效，且 bin.version 匹配后永远不会自动修复。
fn is_non_empty_binary(path: &std::path::Path) -> bool {
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.len() > 0)
        .unwrap_or(false)
}

fn source_bin() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    // 开发模式：独立 crate 自己的 target 目录（与 Voidnix 隔离）。
    // 共享 target 目录时 Voidnix 的 cargo build 会截断 binary（rustc 链接器行为），
    // 独立 target 彻底避免此问题。CARGO_MANIFEST_DIR 编译时求值为 src-tauri/ 绝对路径。
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if let Some(project_root) = manifest_dir.parent() {
        let p = project_root
            .join("extensions/zsh-autosuggestions/native/target/debug")
            .join(BIN_NAME);
        if is_non_empty_binary(&p) {
            return Some(p);
        }
    }
    // release 模式：Resources/（tauri bundle.resources 打包）
    // .app/Contents/MacOS/Voidnix → .app/Contents/Resources/
    if let Some(resources) = exe
        .parent()
        .and_then(|d| d.parent())
        .map(|d| d.join("Resources"))
    {
        let p = resources.join(BIN_NAME);
        if is_non_empty_binary(&p) {
            return Some(p);
        }
    }
    None
}

fn cache_path(app: &AppHandle) -> PathBuf {
    ext_dir(app).join("index.zsh")
}

fn signals_path(app: &AppHandle) -> PathBuf {
    ext_dir(app).join("signals.log")
}

/// .zshrc 中是否已有 marker 行（= 是否已启用）。
/// 以 .zshrc 行为启用判据：marker 行存在 = 用户 shell 会 source init = 补全生效，
/// 是启用的真实物理标志，无需额外镜像文件。
fn is_zshrc_enabled() -> bool {
    let Some(home) = dirs::home_dir() else {
        return false;
    };
    let content = std::fs::read_to_string(home.join(".zshrc")).unwrap_or_default();
    crate::runtime::shell_rc::has_marker(&content, ZSHRC_SCOPE)
}

/// 记录已部署 binary 版本号的标志文件，与 binary 同目录。
fn version_file(app: &AppHandle) -> PathBuf {
    ext_dir(app).join("bin.version")
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
        // source 不可用（编译失败留空文件等）：dest 有效才认为可用。
        // 仅 exists() 不够——0 字节的 dest 同样无效。
        return is_non_empty_binary(&dest);
    };
    install_bin_to(&source, &dest, &version_file(app))
}

/// install_bin 的纯路径参数内核，便于单测。
fn install_bin_to(
    source: &std::path::Path,
    dest: &std::path::Path,
    version_path: &std::path::Path,
) -> bool {
    // 版本匹配 + dest 有效才跳过。dest 为 0 字节占位时强制重复制，
    // 修复 disable→enable 后 binary 损坏无法自愈的问题。
    if is_non_empty_binary(dest) && read_version_at(version_path) == BIN_VERSION {
        return true;
    }

    if let Some(parent) = dest.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::copy(source, dest) {
        Ok(_) => {
            // 确保可执行权限（防御：源文件经过 bundle.resources 等链路可能丢失可执行位）
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(meta) = std::fs::metadata(dest) {
                    let mut perms = meta.permissions();
                    perms.set_mode(0o755);
                    let _ = std::fs::set_permissions(dest, perms);
                }
            }
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

/// 单次 upsert：marker + export 行（runtime/shell_rc 统一约定）。
fn write_zshrc_line(app: &AppHandle) -> Result<(), String> {
    let Some(home) = dirs::home_dir() else {
        return Err("无法定位 home 目录".into());
    };
    let zshrc = home.join(".zshrc");
    let body = build_zshrc_body(&ext_dir(app));
    crate::runtime::shell_rc::upsert_block(&zshrc, ZSHRC_SCOPE, &body).map(|_| ())
}

/// 从 .zshrc 移除本扩展块。
fn remove_zshrc_line() -> Result<(), String> {
    let Some(home) = dirs::home_dir() else {
        return Err("无法定位 home 目录".into());
    };
    let zshrc = home.join(".zshrc");
    crate::runtime::shell_rc::remove_block(&zshrc, ZSHRC_SCOPE).map(|_| ())
}

/// body 行：export ZSH_AS_DIR + eval init（无 marker，由 shell_rc 拼接）。
/// 子路径（bin/cache/signals）由 init.zsh 从 ZSH_AS_DIR 内部 derive。
fn build_zshrc_body(dir: &std::path::Path) -> String {
    format!(
        "export ZSH_AS_DIR={dir}; eval \"$( \"$ZSH_AS_DIR/bin/{bin}\" init )\"",
        dir = crate::runtime::shell_rc::quote_shell(&dir.display().to_string()),
        bin = BIN_NAME,
    )
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
            write_zshrc_line(&app)?;
        } else {
            // 清理运行时数据（可重建），保留 binary 避免反复复制
            let _ = std::fs::remove_file(cache_path(&app));
            let _ = std::fs::remove_file(signals_path(&app));
            remove_zshrc_line()?;
            // 清理 .zshrc 备份文件（disable 后不再需要，避免残留）
            if let Some(home) = dirs::home_dir() {
                let _ = std::fs::remove_file(home.join(".zshrc.voidnix-bak"));
            }
        }

        log::info!("zsh-as enabled={}", enabled);
        Ok(())
    })
    .await
    .map_err(|e| format!("后台任务失败: {e}"))?
}

pub struct ZshAutosuggestionsExtension;

#[async_trait::async_trait]
impl Extension for ZshAutosuggestionsExtension {
    fn id(&self) -> &'static str {
        "zsh-autosuggestions"
    }

    async fn setup(&self, app: &AppHandle) -> tauri::Result<()> {
        let _guard = lock();
        // 以 .zshrc marker 行为启用判据（行存在 = 上次 enable 写入且未被 disable 移除）
        if !is_zshrc_enabled() {
            return Ok(());
        }
        install_bin(app);
        // 幂等刷新 .zshrc 行：升级后若行格式变化，已启用用户自动更新。
        // 启动阶段错误不阻塞（log 即可），用户主动 toggle 才反馈错误。
        if let Err(e) = write_zshrc_line(app) {
            log::warn!("zsh-as: setup write_zshrc_line: {e}");
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
        let dir =
            std::env::temp_dir().join(format!("zsh-as-mod-{label}-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn build_zshrc_body_format() {
        let dir = std::path::Path::new("/u/Lib/x/extensions/zsh-autosuggestions");
        let body = build_zshrc_body(dir);
        assert_eq!(
            body,
            "export ZSH_AS_DIR='/u/Lib/x/extensions/zsh-autosuggestions'; eval \"$( \"$ZSH_AS_DIR/bin/zsh-autosuggestions\" init )\""
        );
        let full = format!(
            "{}\n{body}",
            crate::runtime::shell_rc::marker_line(ZSHRC_SCOPE)
        );
        assert!(full.starts_with("# voidnix zsh-autosuggestions\n"));
    }

    #[test]
    fn upsert_via_shell_rc() {
        let dir = tmp_dir("shell-rc");
        let zshrc = dir.join(".zshrc");
        std::fs::write(&zshrc, "alias ll='ls -la'\n").unwrap();
        let body = build_zshrc_body(std::path::Path::new("/app/dir"));
        assert!(crate::runtime::shell_rc::upsert_block(&zshrc, ZSHRC_SCOPE, &body).unwrap());
        assert!(!crate::runtime::shell_rc::upsert_block(&zshrc, ZSHRC_SCOPE, &body).unwrap());
        let text = std::fs::read_to_string(&zshrc).unwrap();
        assert!(text.contains("alias ll="));
        assert!(text.contains("# voidnix zsh-autosuggestions"));
        assert!(text.contains("ZSH_AS_DIR='/app/dir'"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_skips_when_version_matches() {
        let dir = tmp_dir("install-match");
        let src = dir.join("src.bin");
        let dest = dir.join("dest.bin");
        let ver = dir.join("bin.version");
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
        let ver = dir.join("bin.version");
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
        let ver = dir.join("bin.version");
        std::fs::write(&src, b"SRC").unwrap();

        assert!(install_bin_to(&src, &dest, &ver), "absent → copy");
        assert!(dest.exists());
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "SRC");
        assert_eq!(
            std::fs::read_to_string(&ver).unwrap(),
            BIN_VERSION.to_string()
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_recopies_when_dest_empty_placeholder() {
        // cargo build 失败/中断可能留下 0 字节占位；版本号匹配也必须重复制，
        // 否则 disable→enable 后空 binary 永远无法自愈。
        let dir = tmp_dir("install-empty");
        let src = dir.join("src.bin");
        let dest = dir.join("dest.bin");
        let ver = dir.join("bin.version");
        std::fs::write(&src, b"FRESH").unwrap();
        std::fs::write(&dest, b"").unwrap(); // 0 字节占位
        std::fs::write(&ver, BIN_VERSION.to_string()).unwrap();

        assert!(install_bin_to(&src, &dest, &ver), "empty dest → recopy");
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "FRESH");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
