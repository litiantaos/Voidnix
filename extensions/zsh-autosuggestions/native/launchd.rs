//! LaunchAgent 管理：daemon 由 launchd 托管而非 zsh spawn。
//!
//! 彻底解决：
//!   1) 终端首次启动时 daemon 进程被 zsh spawn 导致的标题栏闪烁
//!   2) 关闭终端弹"是否关闭进程"
//!   3) daemon 崩溃后无人重启
//!
//! launchd 提供 KeepAlive（崩溃自动重启）+ RunAtLoad（用户登录自动起）。
//! Tauri app 仅负责装/卸 plist 和 bootstrap/bootout，不参与 daemon 生命周期。

use std::path::PathBuf;
use tauri::{AppHandle, Manager};

use super::{app_daemon_dir, installed_bin_path};

/// 派生 LaunchAgent label。
/// 数据目录形如 `.../Application Support/<bundle-id>[.dev]/extensions/zsh-autosuggestions`，
/// 取 `<bundle-id>` 部分（dev 模式带 `.dev` 后缀），保证 dev/release LaunchAgent 独立。
pub(super) fn launch_agent_label(app: &AppHandle) -> String {
    let data_dir = app.path().app_data_dir().ok();
    let bundle_dir = data_dir
        .as_ref()
        .and_then(|d| d.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "com.litiantao.voidnix".to_string());
    format!("{}.zsh-as", bundle_dir)
}

/// plist 文件路径：`~/Library/LaunchAgents/<label>.plist`。
fn launch_plist_path(app: &AppHandle) -> Option<PathBuf> {
    let label = launch_agent_label(app);
    dirs::home_dir().map(|h| {
        let dir = h.join("Library").join("LaunchAgents");
        let _ = std::fs::create_dir_all(&dir);
        dir.join(format!("{}.plist", label))
    })
}

/// launchd 域：`gui/<uid>`。
fn launch_domain() -> String {
    let uid = unsafe { libc::geteuid() };
    format!("gui/{}", uid)
}

/// 生成 LaunchAgent plist 内容。
fn render_plist(app: &AppHandle) -> Option<String> {
    let label = launch_agent_label(app);
    let bin = installed_bin_path(app);
    let data_dir = app_daemon_dir(app);
    Some(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{bin}</string>
        <string>daemon</string>
    </array>
    <key>EnvironmentVariables</key>
    <dict>
        <key>ZSH_AS_DATA_DIR</key>
        <string>{data_dir}</string>
    </dict>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>/dev/null</string>
    <key>StandardErrorPath</key>
    <string>/dev/null</string>
    <key>ProcessType</key>
    <string>Background</string>
</dict>
</plist>
"#,
        label = label,
        bin = bin.display(),
        data_dir = data_dir.display(),
    ))
}

/// 写 plist。返回是否实际发生了写操作（首次写或内容变化）。
pub(super) fn write_plist(app: &AppHandle) -> bool {
    let (path, content) = match (launch_plist_path(app), render_plist(app)) {
        (Some(p), Some(c)) => (p, c),
        _ => return false,
    };
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    if existing == content {
        return false;
    }
    std::fs::write(&path, content).is_ok()
}

/// launchd 是否已加载该 agent。
pub(super) fn is_loaded(app: &AppHandle) -> bool {
    let label = launch_agent_label(app);
    let domain = launch_domain();
    std::process::Command::new("launchctl")
        .args(["print", &format!("{}/{}", domain, label)])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// bootstrap（加载并启动）。如果已加载会失败，所以调用前应先 bootout。
fn bootstrap(app: &AppHandle) -> bool {
    let path = match launch_plist_path(app) {
        Some(p) => p,
        None => return false,
    };
    let domain = launch_domain();
    std::process::Command::new("launchctl")
        .args(["bootstrap", &domain, &path.display().to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// bootout（停止并卸载）。未加载时静默失败。
fn bootout(app: &AppHandle) {
    let label = launch_agent_label(app);
    let domain = launch_domain();
    let _ = std::process::Command::new("launchctl")
        .args(["bootout", &format!("{}/{}", domain, label)])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// 删除 plist 文件（disable 时调用）。
fn remove_plist(app: &AppHandle) {
    if let Some(path) = launch_plist_path(app) {
        let _ = std::fs::remove_file(path);
    }
}

/// 完整启用流程：写 plist → bootstrap（必要时跳过，已加载就不重复）。
pub(super) fn ensure_running(app: &AppHandle) {
    write_plist(app);
    if is_loaded(app) {
        return;
    }
    bootstrap(app);
}

/// 完整重启流程：bootout → bootstrap（用于 binary 版本变化时强制重启 daemon）。
pub(super) fn force_restart(app: &AppHandle) {
    write_plist(app);
    bootout(app);
    // bootout 后等一下让 launchd 真正释放资源
    std::thread::sleep(std::time::Duration::from_millis(50));
    bootstrap(app);
}

/// 完整卸载流程：bootout + 删 plist。
pub(super) fn uninstall(app: &AppHandle) {
    bootout(app);
    remove_plist(app);
}
