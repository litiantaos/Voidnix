use std::sync::atomic::{AtomicBool, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(false);

const DAEMON_BIN_NAME: &str = "zsh-autosuggestions";

fn app_daemon_dir(app: &tauri::AppHandle) -> std::path::PathBuf {
    crate::infra::path::zsh_daemon_dir(app)
}

fn installed_bin_path(app: &tauri::AppHandle) -> std::path::PathBuf {
    crate::infra::path::zsh_daemon_bin_path(app)
}

fn source_bin_path() -> Option<std::path::PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(DAEMON_BIN_NAME)))
        .filter(|p| p.exists())
}

fn flag_path(app: &tauri::AppHandle) -> std::path::PathBuf {
    crate::infra::path::zsh_daemon_flag_path(app)
}

fn install_daemon_bin(app: &tauri::AppHandle) -> bool {
    let dest = installed_bin_path(app);
    if dest.exists() {
        return true;
    }

    let source = match source_bin_path() {
        Some(s) => s,
        None => return false,
    };

    if let Some(parent) = dest.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    std::fs::copy(&source, &dest).is_ok()
}

fn remove_old_zshrc_line(zshrc: &std::path::Path) {
    let Ok(content) = std::fs::read_to_string(zshrc) else {
        return;
    };

    let new_content: String = content
        .lines()
        .filter(|line| !line.contains("voidnix zsh-autosuggestions"))
        .collect::<Vec<&str>>()
        .join("\n");

    if new_content != content {
        let _ = std::fs::write(zshrc, new_content);
    }
}

fn remove_zshrc_line() {
    if let Some(home) = dirs::home_dir() {
        let zshrc = home.join(".zshrc");
        remove_old_zshrc_line(&zshrc);
    }
}

fn write_zshrc_line(app: &tauri::AppHandle) {
    let Some(home) = dirs::home_dir() else { return };
    let zshrc = home.join(".zshrc");

    remove_old_zshrc_line(&zshrc);

    let bin = installed_bin_path(app);
    let line = format!(
        "eval \"$('{}' init)\"  # voidnix zsh-autosuggestions",
        bin.display()
    );

    if let Ok(content) = std::fs::read_to_string(&zshrc) {
        if content.contains(&line) {
            return;
        }
    }

    let _ = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&zshrc)
        .and_then(|mut f| {
            use std::io::Write;
            writeln!(f, "\n{}", line)
        });
}

#[tauri::command]
pub fn set_zsh_autosuggestions_enabled(app: tauri::AppHandle, enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);

    let dir = app_daemon_dir(&app);
    let _ = std::fs::create_dir_all(&dir);

    let flag = flag_path(&app);
    if enabled {
        install_daemon_bin(&app);
        let _ = std::fs::write(&flag, b"1");
        write_zshrc_line(&app);
    } else {
        let _ = std::fs::remove_file(&flag);
        remove_zshrc_line();
        kill_daemon();
    }

    log::info!("zsh-autosuggestions enabled={}", enabled);
}

#[tauri::command]
pub fn zsh_autosuggestions_status(app: tauri::AppHandle) -> serde_json::Value {
    let installed = check_zshrc_installed();
    let running = check_daemon_running(&app);
    let enabled = ENABLED.load(Ordering::Relaxed);

    serde_json::json!({
        "installed": installed,
        "daemonRunning": running,
        "enabled": enabled,
    })
}

fn check_zshrc_installed() -> bool {
    if let Some(home) = dirs::home_dir() {
        let zshrc = home.join(".zshrc");
        if let Ok(content) = std::fs::read_to_string(&zshrc) {
            return content.contains("voidnix zsh-autosuggestions");
        }
    }
    false
}

fn check_daemon_running(app: &tauri::AppHandle) -> bool {
    let bin = installed_bin_path(app);
    if !bin.exists() {
        return false;
    }
    std::process::Command::new(&bin)
        .arg("ping")
        .env("ZSH_AS_DATA_DIR", app_daemon_dir(app).display().to_string())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn kill_daemon() {
    let _ = std::process::Command::new("pkill")
        .args(["-f", &format!("{} daemon", DAEMON_BIN_NAME)])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new("zsh_autosuggestions")
        .setup(|app, _api| {
            let flag = flag_path(app);
            if flag.exists() {
                ENABLED.store(true, Ordering::Relaxed);
            }
            Ok(())
        })
        .build()
}
