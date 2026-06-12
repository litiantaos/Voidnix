use crate::core::tier1::Tier1Extension;
use std::sync::atomic::{AtomicBool, Ordering};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

static ENABLED: AtomicBool = AtomicBool::new(false);

const DAEMON_BIN_NAME: &str = "zsh-autosuggestions";

fn app_daemon_dir(app: &AppHandle) -> PathBuf {
    let dir = app.path().app_data_dir().unwrap_or_else(|_| PathBuf::from("."))
        .join("extensions")
        .join("zsh-autosuggestions");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn installed_bin_path(app: &AppHandle) -> PathBuf {
    app_daemon_dir(app).join("bin").join(DAEMON_BIN_NAME)
}

fn source_bin_path() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(DAEMON_BIN_NAME)))
        .filter(|p| p.exists())
}

fn flag_path(app: &AppHandle) -> PathBuf {
    app_daemon_dir(app).join("enabled")
}

fn import_marker_path(app: &AppHandle) -> PathBuf {
    app_daemon_dir(app).join(".imported")
}

fn version_marker_path(app: &AppHandle) -> PathBuf {
    app_daemon_dir(app).join(".installed_version")
}

fn install_daemon_bin(app: &AppHandle) -> bool {
    let dest = installed_bin_path(app);
    let source = match source_bin_path() {
        Some(s) => s,
        None => return dest.exists(),
    };

    let current_version = env!("CARGO_PKG_VERSION");
    let version_ok =
        std::fs::read_to_string(version_marker_path(app)).is_ok_and(|v| v == current_version);
    let mtime_ok = !std::fs::metadata(&source)
        .and_then(|m| m.modified())
        .ok()
        .zip(std::fs::metadata(&dest).and_then(|m| m.modified()).ok())
        .is_some_and(|(src, dst)| src > dst);

    if dest.exists() && version_ok && mtime_ok {
        return true;
    }

    if let Some(parent) = dest.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    if std::fs::copy(&source, &dest).is_ok() {
        let _ = std::fs::write(version_marker_path(app), current_version);
        true
    } else {
        false
    }
}

fn import_history(app: &AppHandle) {
    let bin = installed_bin_path(app);
    if !bin.exists() {
        return;
    }
    let result = std::process::Command::new(&bin)
        .arg("import")
        .env("ZSH_AS_DATA_DIR", app_daemon_dir(app).display().to_string())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output();
    match result {
        Ok(out) if out.status.success() => {
            log::info!(
                "zsh-autosuggestions: {}",
                String::from_utf8_lossy(&out.stdout).trim()
            );
        }
        Ok(out) => {
            log::warn!(
                "zsh-autosuggestions import failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Err(e) => {
            log::warn!("zsh-autosuggestions import error: {}", e);
        }
    }
}

fn remove_old_zshrc_line(zshrc: &std::path::Path) {
    let Ok(content) = std::fs::read_to_string(zshrc) else {
        return;
    };

    let trailing_nl = content.ends_with('\n');
    let new_content: String = content
        .lines()
        .filter(|line| !line.contains("voidnix zsh-autosuggestions"))
        .collect::<Vec<&str>>()
        .join("\n");

    let final_content = if trailing_nl && !new_content.is_empty() {
        format!("{}\n", new_content)
    } else {
        new_content
    };

    if final_content != content {
        let _ = std::fs::write(zshrc, final_content);
    }
}

fn remove_zshrc_line() {
    if let Some(home) = dirs::home_dir() {
        let zshrc = home.join(".zshrc");
        remove_old_zshrc_line(&zshrc);
    }
}

fn write_zshrc_line(app: &AppHandle) {
    let Some(home) = dirs::home_dir() else { return };
    let zshrc = home.join(".zshrc");

    remove_old_zshrc_line(&zshrc);

    let bin = installed_bin_path(app);
    let line = format!(
        "eval \"$('{}' init)\"  # voidnix zsh-autosuggestions",
        bin.display()
    );

    let content = std::fs::read_to_string(&zshrc).unwrap_or_default();
    if content.contains(&line) {
        return;
    }

    let _ = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&zshrc)
        .and_then(|mut f| {
            use std::io::Write;
            if !content.is_empty() {
                if !content.ends_with('\n') {
                    writeln!(f)?;
                }
                writeln!(f)?;
            }
            writeln!(f, "{}", line)
        });
}

#[tauri::command]
pub fn set_zsh_autosuggestions_enabled(app: AppHandle, enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);

    let dir = app_daemon_dir(&app);
    let _ = std::fs::create_dir_all(&dir);

    let flag = flag_path(&app);
    if enabled {
        install_daemon_bin(&app);
        if !import_marker_path(&app).exists() {
            import_history(&app);
            let _ = std::fs::write(import_marker_path(&app), b"1");
        }
        let _ = std::fs::write(&flag, b"1");
        write_zshrc_line(&app);
    } else {
        let _ = std::fs::remove_file(&flag);
        remove_zshrc_line();
        kill_daemon();
    }

    log::info!("zsh-autosuggestions enabled={}", enabled);
}

fn kill_daemon() {
    let _ = std::process::Command::new("pkill")
        .args(["-f", &format!("{} daemon", DAEMON_BIN_NAME)])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// Zsh autosuggestions 扩展。
pub struct Plugin;

impl Tier1Extension for Plugin {
    fn id(&self) -> &'static str {
        "zsh-autosuggestions"
    }

    fn on_setup(&self, app: &AppHandle) -> tauri::Result<()> {
        let flag = flag_path(app);
        if flag.exists() {
            ENABLED.store(true, Ordering::Relaxed);

            // Auto-update daemon binary on app launch if version changed or binary missing
            let current_version = env!("CARGO_PKG_VERSION");
            let installed_version =
                std::fs::read_to_string(version_marker_path(app)).unwrap_or_default();
            if installed_version.trim() != current_version || !installed_bin_path(app).exists() {
                if install_daemon_bin(app) {
                    kill_daemon();
                    log::info!(
                        "zsh-autosuggestions: daemon binary updated to v{}",
                        current_version
                    );
                }
            }
        }
        Ok(())
    }
}
