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
/// 输出重定向到 mihomo.log（启动失败时可查日志诊断）。
pub async fn spawn_root(app: &AppHandle, params: &super::core::RunParams) -> Result<(), String> {
    let bin = super::core::ensure_bin(app).await?;
    super::core::ensure_geo_files(app).await?;
    let yaml = super::subscription::build_run_config(app, params)?;
    std::fs::write(super::core::run_config_path(app)?, yaml).map_err(|e| e.to_string())?;
    let dir = ext_data_dir(app, "proxy")?;
    let pidfile = dir.join("mihomo.pid");
    let logfile = dir.join("mihomo.log");
    let cmd = format!(
        "{} -d {} >{} 2>&1 & echo $! > {}",
        shell_quote(&bin.display().to_string()),
        shell_quote(&dir.display().to_string()),
        shell_quote(&logfile.display().to_string()),
        shell_quote(&pidfile.display().to_string()),
    );
    let script = format!("do shell script \"{cmd}\" with administrator privileges");
    run_osascript(&script)
}

/// 提权重启 root mihomo：杀旧 + 启新（单次 osascript 提权，免多次密码框）。
///
/// SIGTERM → 轮询确认退出（至多 2s，撤 TUN/路由）→ SIGKILL 兜底 → sleep 1（端口/utun 释放）
/// → 启新。用于遗留进程状态不可信时（secret 不匹配/TUN 已激活）的干净重启——比 stop_root +
/// spawn_root 两次独立提权更可靠（单次密码框 + 杀启原子化，杜绝端口占用 race）。
/// SIGTERM 阶段用轮询而非固定 sleep：正常退出（几百 ms）更快进入下一阶段，异常给满 2s 再强杀，
/// 避免 mihomo 未及撤销 utun 就被 SIGKILL 致残留虚拟网卡挡道新进程 TUN 创建。
pub async fn restart_root(app: &AppHandle, params: &super::core::RunParams) -> Result<(), String> {
    let bin = super::core::ensure_bin(app).await?;
    super::core::ensure_geo_files(app).await?;
    let yaml = super::subscription::build_run_config(app, params)?;
    std::fs::write(super::core::run_config_path(app)?, yaml).map_err(|e| e.to_string())?;
    let dir = ext_data_dir(app, "proxy")?;
    let pidfile = dir.join("mihomo.pid");
    let logfile = dir.join("mihomo.log");
    let bin_q = shell_quote(&bin.display().to_string());
    let dir_q = shell_quote(&dir.display().to_string());
    let pidfile_q = shell_quote(&pidfile.display().to_string());
    let logfile_q = shell_quote(&logfile.display().to_string());
    let cmd = format!(
        "for p in $(ps -eo pid,args | grep -F {bin_q} | grep -v grep | awk '{{print $1}}'); do \
            kill $p 2>/dev/null; \
         done; \
         i=0; \
         while ps -eo pid,args | grep -F {bin_q} | grep -v grep | grep -q .; do \
            i=$((i+1)); \
            if [ $i -gt 10 ]; then break; fi; \
            sleep 0.2; \
         done; \
         for p in $(ps -eo pid,args | grep -F {bin_q} | grep -v grep | awk '{{print $1}}'); do \
            kill -9 $p 2>/dev/null; \
         done; \
         sleep 1; \
         rm -f {pidfile_q}; \
         {bin_q} -d {dir_q} >{logfile_q} 2>&1 & echo $! > {pidfile_q}"
    );
    let script = format!("do shell script \"{cmd}\" with administrator privileges");
    run_osascript(&script)
}

/// 提权停止 root mihomo（kill root 进程需 root）。
///
/// 优先按 pidfile 的 PID 优雅停：SIGTERM → 轮询 `kill -0` 确认退出（3s 内）→ 超时 SIGKILL。
/// 再按 mihomo binary 完整路径（含 bundle-id 数据目录，全局唯一）扫杀所有残留进程，
/// 防止 pidfile 记录与实际 PID 脱节、或多次 spawn 累积孤儿进程（旧实现仅 kill pidfile 的
/// PID 且 `2>/dev/null` 吞错，导致关闭「表面成功」而 root mihomo + utun 网卡 + 路由全残留，
/// 流量被持续代理）。末尾验证确无残留，否则报错——让前端感知关闭失败而非假装成功。
/// `grep -v grep` 排除 grep 自身（其命令行亦含该路径），防误杀当前 shell。
pub fn stop_root(app: &AppHandle) -> Result<(), String> {
    let dir = ext_data_dir(app, "proxy")?;
    let bin = dir.join("mihomo");
    let pidfile = dir.join("mihomo.pid");
    let bin_q = shell_quote(&bin.display().to_string());
    let pidfile_q = shell_quote(&pidfile.display().to_string());
    let cmd = format!(
        "if [ -f {pidfile_q} ]; then \
            PID=$(cat {pidfile_q}); \
            kill $PID 2>/dev/null; \
            i=0; \
            while kill -0 $PID 2>/dev/null; do \
                i=$((i+1)); \
                if [ $i -gt 15 ]; then kill -9 $PID 2>/dev/null; break; fi; \
                sleep 0.2; \
            done; \
         fi; \
         for p in $(ps -eo pid,args | grep -F {bin_q} | grep -v grep | awk '{{print $1}}'); do \
            kill -9 $p 2>/dev/null; \
         done; \
         rm -f {pidfile_q}; \
         if ps -eo pid,args | grep -F {bin_q} | grep -v grep | grep -q .; then exit 1; else exit 0; fi"
    );
    let script = format!("do shell script \"{cmd}\" with administrator privileges");
    run_osascript(&script)?;
    let _ = std::fs::remove_file(&pidfile);
    Ok(())
}

/// 执行 osascript，识别用户取消（-128）。
///
/// 调用期间暂停 click-outside 检测：`with administrator privileges` 弹出的 SecurityAgent
/// 授权框在主窗口外，用户点击（输密码/确认）会被判为 click-outside 触发 hideWindow，
/// 导致授权未完成窗口就关闭。
fn run_osascript(script: &str) -> Result<(), String> {
    crate::platform::click_monitor::suppress(true);
    let result = (|| {
        let out = Command::new("osascript")
            .args(["-e", script])
            .output()
            .map_err(|e| format!("osascript 调用失败: {e}"))?;
        if out.status.success() {
            return Ok(());
        }
        let err = String::from_utf8_lossy(&out.stderr);
        if err.contains("-128") || err.contains("User canceled") || err.contains("user canceled") {
            return Err("已取消授权".to_string());
        }
        Err(err.trim().to_string())
    })();
    crate::platform::click_monitor::suppress(false);
    result
}
