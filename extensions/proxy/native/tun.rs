use crate::runtime::storage::ext_data_dir;
use std::process::Command;
use tauri::AppHandle;

/// 单引号 shell 转义：app data dir 含空格（Application Support），须引号包裹。
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// 通过 osascript 提权后台启动 mihomo（TUN 需 root 创建虚拟网卡 + auto-route）。
///
/// `do shell script "... & echo $! > pidfile" with administrator privileges`：
/// mihomo 后台 detach（PPID→1），$! 写入 pidfile 供热重启/停止用。弹出系统授权对话框。
pub async fn spawn_root(app: &AppHandle, params: &super::core::RunParams) -> Result<(), String> {
    let (bin, dir) = super::core::prepare(app, params).await?;
    let pidfile = dir.join("mihomo.pid");
    let cmd = format!(
        "{} -d {} >/dev/null 2>&1 & echo $! > {}",
        shell_quote(&bin.display().to_string()),
        shell_quote(&dir.display().to_string()),
        shell_quote(&pidfile.display().to_string()),
    );
    let script = format!("do shell script \"{cmd}\" with administrator privileges");
    run_osascript(&script)
}

/// 提权停止 root mihomo（kill root 进程需 root）。读取 pidfile 取 PID，kill 后清理。
pub fn stop_root(app: &AppHandle) -> Result<(), String> {
    let dir = ext_data_dir(app, "proxy")?;
    let pidfile = dir.join("mihomo.pid");
    let pid = std::fs::read_to_string(&pidfile)
        .map_err(|e| format!("读取 pid 失败: {e}"))?
        .trim()
        .to_string();
    let cmd = format!(
        "kill {pid} 2>/dev/null; rm -f {}",
        shell_quote(&pidfile.display().to_string())
    );
    let script = format!("do shell script \"{cmd}\" with administrator privileges");
    run_osascript(&script)?;
    let _ = std::fs::remove_file(&pidfile);
    Ok(())
}

/// 执行 osascript，识别用户取消（-128）。
fn run_osascript(script: &str) -> Result<(), String> {
    let out = Command::new("osascript")
        .args(["-e", script])
        .output()
        .map_err(|e| format!("osascript 调用失败: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    if err.contains("-128") || err.contains("User canceled") || err.contains("user canceled") {
        return Err("用户取消授权".to_string());
    }
    Err(err.trim().to_string())
}
