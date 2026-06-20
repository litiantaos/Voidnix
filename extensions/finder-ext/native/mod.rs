use crate::runtime::registry::Extension;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use notify::{recommended_watcher, Event, EventKind, RecursiveMode, Watcher};
use tauri::AppHandle;
use tauri::Manager;

/// Guards whether the watcher should process incoming commands.
/// Set to false when the user disables the extension in settings.
static FINDER_EXT_ENABLED: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub fn set_finder_ext_enabled(app: tauri::AppHandle, enabled: bool) {
    FINDER_EXT_ENABLED.store(enabled, Ordering::Relaxed);
    log::info!("finder_ext enabled={}", enabled);

    let flag_path = command_dir(&app).join("enabled");
    if enabled {
        let _ = fs::write(&flag_path, b"1");
    } else {
        let _ = fs::remove_file(&flag_path);
    }
}

/// Maximum size of a single command JSON file (1 MB).
const MAX_COMMAND_SIZE: u64 = 1_048_576;

/// System path prefixes that are always rejected regardless of other checks.
struct CommandHandler {
    handle: AppHandle,
}

impl notify::EventHandler for CommandHandler {
    fn handle_event(&mut self, event: Result<Event, notify::Error>) {
        let Ok(event) = event else { return };
        // The extension writes commands with atomic write (write .tmp then
        // rename to .json). On macOS that surfaces as Modify(Name(To)),
        // not Create(_). Check both kinds and guard with is_file().
        if !matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
            return;
        }
        for path in &event.paths {
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if !path.is_file() {
                continue;
            }
            process_command(path, &self.handle);
        }
    }
}

/// IPC directory shared between the sandboxed extension and this main app.
fn command_dir(app: &AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("extensions")
        .join("finder-ext")
        .join("commands");
    let _ = fs::create_dir_all(&dir);
    dir
}

/// 命令注册（局部 invoke_handler，§2.8）。
pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new("finder-ext").build()
}

/// Finder 扩展。
pub struct FinderExtExtension;

#[async_trait::async_trait]
impl Extension for FinderExtExtension {
    fn id(&self) -> &'static str {
        "finder-ext"
    }

    async fn setup(&self, app_handle: &AppHandle) -> tauri::Result<()> {
        init_finder_ext(app_handle.clone());
        Ok(())
    }
}

fn init_finder_ext(app_handle: AppHandle) {
    let cmd_dir = command_dir(&app_handle);

    // Remove stale .tmp files; replay any .json queued before we started.
    if let Ok(entries) = fs::read_dir(&cmd_dir) {
        let mut pending: Vec<PathBuf> = Vec::new();
        for entry in entries.flatten() {
            let p = entry.path();
            match p.extension().and_then(|e| e.to_str()) {
                Some("tmp") => {
                    let _ = fs::remove_file(&p);
                }
                Some("json") => pending.push(p),
                _ => {}
            }
        }
        for p in pending {
            process_command(&p, &app_handle);
        }
    }

    let shutdown_flag = Arc::new(AtomicBool::new(false));

    if let Some(window) = app_handle.get_webview_window("main") {
        let flag = shutdown_flag.clone();
        window.on_window_event(move |event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                flag.store(true, Ordering::SeqCst);
            }
        });
    }

    std::thread::spawn({
        let watch_dir = cmd_dir.clone();
        let flag = shutdown_flag;
        move || {
            let handler = CommandHandler {
                handle: app_handle.clone(),
            };
            let mut watcher = match recommended_watcher(handler) {
                Ok(w) => w,
                Err(e) => {
                    log::error!("Failed to create finder ext watcher: {:?}", e);
                    return;
                }
            };
            if let Err(e) = watcher.watch(&watch_dir, RecursiveMode::NonRecursive) {
                log::error!("Failed to watch cmd dir: {:?}", e);
                return;
            }
            log::info!("Finder ext watcher started on {:?}", watch_dir);
            while !flag.load(Ordering::SeqCst) {
                std::thread::park_timeout(Duration::from_millis(500));
            }
            drop(watcher);
        }
    });
}

fn process_command(path: &Path, handle: &AppHandle) {
    if !FINDER_EXT_ENABLED.load(Ordering::Relaxed) {
        let _ = fs::remove_file(path);
        return;
    }
    let meta = match fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return,
    };
    if meta.len() > MAX_COMMAND_SIZE {
        let _ = fs::remove_file(path);
        return;
    }

    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to read command file {:?}: {}", path, e);
            return;
        }
    };
    let _ = fs::remove_file(path);

    let cmd: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            log::error!("Invalid command JSON: {}", e);
            return;
        }
    };

    let action = cmd.get("action").and_then(|v| v.as_str()).unwrap_or("");
    // Reject obviously-bogus inputs before we do anything expensive.
    // The IPC file is an append-only trust boundary; cap everything.
    if action.len() > 64 {
        log::warn!(
            "finder_ext: action too long ({} bytes), dropping",
            action.len()
        );
        return;
    }
    let target = cmd.get("target").and_then(|v| v.as_str()).unwrap_or("");
    if target.len() > 4096 {
        log::warn!(
            "finder_ext: target too long ({} bytes), dropping",
            target.len()
        );
        return;
    }
    const MAX_PATHS: usize = 256;
    let raw_paths: Vec<String> = cmd
        .get("paths")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .take(MAX_PATHS)
                .filter_map(|v| v.as_str().map(String::from))
                .filter(|s| s.len() <= 4096)
                .collect()
        })
        .unwrap_or_default();

    let paths: Vec<PathBuf> = raw_paths
        .iter()
        .map(PathBuf::from)
        .filter(|p| validate_path(p))
        .collect();

    log::info!(
        "Finder ext: action={} paths={} target.len={}",
        action,
        paths.len(),
        target.len()
    );

    match action {
        "copy_path" => handle_copy_path(handle, &paths, target),
        "open_terminal" => handle_open_terminal(handle, &paths, target),
        "toggle_hidden" => handle_toggle_hidden(),
        "new_file" => handle_new_file(target),
        _ => log::warn!("Unknown finder ext action: {}", action),
    }
}

/// 委托至 platform::path_guard 统一路径校验（拦系统致命路径）。
fn validate_path(path: &Path) -> bool {
    crate::platform::path_guard::validate(path)
}

fn handle_copy_path(_handle: &AppHandle, paths: &[PathBuf], target: &str) {
    let lines: Vec<String> = if !paths.is_empty() {
        paths
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect()
    } else if !target.is_empty() {
        vec![target.to_string()]
    } else {
        return;
    };
    let text = lines.join("\n");
    crate::platform::pasteboard::write_text(&text);
}

fn handle_open_terminal(_handle: &AppHandle, paths: &[PathBuf], target: &str) {
    let dir = if let Some(p) = paths.first() {
        if p.is_dir() {
            p.clone()
        } else {
            p.parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."))
        }
    } else if !target.is_empty() {
        let pb = Path::new(target);
        if !validate_path(pb) {
            return;
        }
        let resolved = pb.canonicalize().unwrap_or_else(|_| pb.to_path_buf());
        if resolved.is_dir() {
            resolved
        } else {
            resolved
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."))
        }
    } else {
        return;
    };

    // 固定使用 Terminal.app。已知限制：不尊重用户偏好的 iTerm2/Warp/Alacritty 等。
    // 未来可读 com.apple.LaunchServices 默认终端 bundle id 或扩展 config 增加配置项。
    let _ = Command::new("open")
        .args(["-b", "com.apple.Terminal", dir.to_string_lossy().as_ref()])
        .spawn();
}

fn handle_toggle_hidden() {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::NSApplicationActivationOptions;

        let ws = objc2_app_kit::NSWorkspace::sharedWorkspace();
        let apps = ws.runningApplications();
        let finder = apps.iter().find(|a| {
            a.bundleIdentifier()
                .map(|b| b.to_string() == "com.apple.finder")
                .unwrap_or(false)
        });
        let Some(finder) = finder else { return };
        let pid = finder.processIdentifier();

        let frontmost_pid = ws.frontmostApplication().map(|a| a.processIdentifier());

        if frontmost_pid != Some(pid) {
            #[allow(deprecated)]
            let options = NSApplicationActivationOptions::ActivateIgnoringOtherApps;
            finder.activateWithOptions(options);
            std::thread::sleep(Duration::from_millis(200));
        }

        crate::platform::input::post_combo("cmd+shift+.", Some(pid));
    }
}

fn handle_new_file(target: &str) {
    let dir = if target.is_empty() {
        dirs::desktop_dir().unwrap_or_else(|| PathBuf::from("."))
    } else {
        let pb = Path::new(target);
        if !validate_path(pb) {
            return;
        }
        let resolved = pb.canonicalize().unwrap_or_else(|_| pb.to_path_buf());
        if !resolved.is_dir() {
            return;
        }
        resolved
    };

    let created_path = {
        let mut counter: u32 = 0;
        loop {
            counter += 1;
            if counter > 10_000 {
                log::error!("new_file: gave up after 10000 attempts in {:?}", dir);
                return;
            }
            let filename = if counter == 1 {
                "Untitled.txt".to_string()
            } else {
                format!("Untitled {}.txt", counter)
            };
            let path = dir.join(&filename);
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(_) => break path,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => {
                    log::error!("Failed to create {:?}: {}", path, e);
                    return;
                }
            }
        }
    };

    reveal_and_rename(&created_path);
}

fn reveal_in_finder(path: &Path) {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::NSWorkspace;
        let path_str = path.to_string_lossy().to_string();
        let ns_path = objc2_foundation::NSString::from_str(&path_str);
        let ws = NSWorkspace::sharedWorkspace();
        let _ = ws.selectFile_inFileViewerRootedAtPath(
            Some(&ns_path),
            &objc2_foundation::NSString::from_str(""),
        );
    }
}

fn reveal_and_rename(path: &Path) {
    reveal_in_finder(path);

    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::NSWorkspace;

        // 等待 Finder 完成选中，再发送 Return 触发重命名
        std::thread::sleep(Duration::from_millis(300));

        let ws = NSWorkspace::sharedWorkspace();
        let finder = ws.runningApplications().iter().find(|a| {
            a.bundleIdentifier()
                .map(|b| b.to_string() == "com.apple.finder")
                .unwrap_or(false)
        });
        if let Some(finder) = finder {
            let pid = finder.processIdentifier();
            // Return 触发 Finder 对选中文件进入重命名模式
            crate::platform::input::post_combo("return", Some(pid));
        }
    }
}

#[tauri::command]
pub fn quit_app(app_handle: AppHandle) {
    // 禁用 Finder 扩展
    FINDER_EXT_ENABLED.store(false, Ordering::Relaxed);
    let flag_path = command_dir(&app_handle).join("enabled");
    let _ = fs::remove_file(&flag_path);

    // 终止 Finder 扩展进程
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::NSRunningApplication;
        let bundle_id = objc2_foundation::NSString::from_str("com.litiantao.voidnix.FinderExt");
        let apps = NSRunningApplication::runningApplicationsWithBundleIdentifier(&bundle_id);
        for app in apps.iter() {
            app.terminate();
        }
    }

    log::info!("Quitting app...");
    app_handle.exit(0);
}

#[tauri::command]
pub fn check_finder_ext_authorized() -> bool {
    let output = Command::new("pluginkit")
        .args([
            "-m",
            "-p",
            "com.apple.FinderSync",
            "-i",
            "com.litiantao.voidnix.FinderExt",
        ])
        .output();
    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout.contains("com.litiantao.voidnix.FinderExt")
        }
        Err(_) => false,
    }
}

#[tauri::command]
pub fn open_extensions_prefs() {
    // macOS 13–15: x-apple.systempreferences:com.apple.ExtensionsPreferences
    // macOS 26+:   the above URL no longer opens the right pane.
    // Using `open /System/Library/PreferencePanes/Extensions.prefPane` also
    // stopped working on macOS 26. The most reliable cross-version approach
    // is to open System Settings and let the user navigate, or use the
    // Login Items & Extensions URL which works on macOS 13+.
    let urls = [
        "x-apple.systempreferences:com.apple.LoginItems-Settings.extension",
        "x-apple.systempreferences:com.apple.ExtensionsPreferences",
    ];
    for url in urls {
        let status = Command::new("open").arg(url).status();
        if status.map(|s| s.success()).unwrap_or(false) {
            return;
        }
    }
    // Last resort: open System Settings root
    let _ = Command::new("open")
        .args(["-b", "com.apple.systempreferences"])
        .spawn();
}
