//! TUN 模式 root 提权：mihomo 经 launchd LaunchDaemon 托管。
//!
//! 首次开启代理时 osascript 提权一次安装 LaunchDaemon（`/Library/LaunchDaemons/<id>.mihomo.plist`），
//! 之后 mihomo 由 launchd 托管常驻（RunAtLoad 开机自启 + KeepAlive 崩溃自愈），Voidnix 全程经
//! controller API 热重载 active/idle config 控制，日常零提权。binary 升级/卸载才再次提权。

use crate::platform::elevate::{self, shell_quote, ElevateError};
use crate::runtime::storage::ext_data_dir;
use std::path::PathBuf;
use std::process::Command;
use tauri::AppHandle;

/// LaunchDaemon label（按 bundle identifier 区分 dev/prod）。
fn daemon_label(app: &AppHandle) -> String {
    format!("{}.mihomo", app.config().identifier)
}

/// 已安装的 plist 路径（`/Library/LaunchDaemons/<label>.plist`）。
fn plist_install_path(label: &str) -> PathBuf {
    PathBuf::from("/Library/LaunchDaemons").join(format!("{label}.plist"))
}

/// plist 是否已安装（`/Library/LaunchDaemons` 全局可读，无需 root）。
pub(crate) fn plist_installed(app: &AppHandle) -> bool {
    plist_install_path(&daemon_label(app)).exists()
}

/// 生成 LaunchDaemon plist（绝对路径；mihomo 以 root 跑，读数据目录的 config.yaml）。
fn generate_plist(label: &str, bin: &str, dir: &str) -> String {
    let log = format!("{dir}/mihomo.log");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{bin}</string>
        <string>-d</string>
        <string>{dir}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>ThrottleInterval</key>
    <integer>30</integer>
    <key>StandardOutPath</key>
    <string>{log}</string>
    <key>StandardErrorPath</key>
    <string>{log}</string>
</dict>
</plist>"#
    )
}

/// 安装 LaunchDaemon 并启动 mihomo（osascript 提权一次）。
///
/// 生成 **idle config**（direct + 无 tun 段）作为 mihomo 启动初始配置——idle 不占 TUN，是安全
/// 默认；开启代理由 `start_core` 随后热重载 active config 完成。
///
/// **冲突处理（三层）**：
/// 1. 安装前探测端口（`port_occupant`）：mixed-port/controller 被别的程序占 → 直接报错不强杀，
///    让用户知情（关别的工具 / 改端口）。dev/prod 默认端口隔离（dev +1 偏移），互不占用。
/// 2. install 脚本只杀**自己的** mihomo（路径含 bundle-id 数据目录），不碰别的实例。
///    bootstrap 后 curl 轮询验 controller 可达（secret 匹配）——mihomo 绑定端口失败时
///    **不退出**（降级运行无监听），pgrep 误判成功；controller API 健康检查才能识别降级实例，
///    是则在同一提权 session 内 bootout + 删 plist——从根源消除 KeepAlive 反复拉起刷日志。
///    install 返回前再从 Voidnix 进程 wait_ready 复验（curl 在 root shell 上下文，与 Voidnix
///    的 reqwest 不同执行上下文，且 mihomo 刚 bootstrap 有初始化抖动窗口，本进程复验为 reload 铺路）。
/// 3. wait_ready 超时后读 mihomo.log + lsof 拼精确诊断（端口占用者 / TUN 冲突）。
pub async fn install_launchdaemon(
    app: &AppHandle,
    params: &super::core::RunParams,
) -> Result<(), String> {
    // 第一层：端口预探测——别的程序占端口则报错（提示用户处理）
    for &port in &[params.mixed_port, params.controller_port] {
        if let Some((pid, command)) = port_occupant(port) {
            if !is_own_mihomo(&command, app) {
                return Err(format!(
                    "{port} 端口被 {}（PID {pid}）占用，请先关闭它或在设置中修改 Voidnix 端口",
                    process_name(&command)
                ));
            }
        }
    }

    let bin = super::core::ensure_bin(app).await?;
    super::core::ensure_geo_files(app).await?;
    // idle config：mihomo 启动不代理（start_core 随后热重载 active 开启代理）
    let idle = super::core::RunParams {
        mode: "direct".into(),
        tun: false,
        ..params.clone()
    };
    let yaml = super::subscription::build_run_config(app, &idle)?;
    std::fs::write(super::core::run_config_path(app)?, yaml).map_err(|e| e.to_string())?;

    let dir = ext_data_dir(app, "proxy")?;
    let label = daemon_label(app);
    let plist = generate_plist(
        &label,
        &bin.display().to_string(),
        &dir.display().to_string(),
    );
    // 临时 plist 写 app data dir（普通权限），提权脚本 cat 到系统目录
    let tmp_plist = dir.join("mihomo-daemon.plist");
    std::fs::write(&tmp_plist, &plist).map_err(|e| e.to_string())?;
    let tmp_q = shell_quote(&tmp_plist.display().to_string());
    let dest_q = shell_quote(&plist_install_path(&label).display().to_string());
    let bin_q = shell_quote(&bin.display().to_string());
    let log_q = shell_quote(&dir.join("mihomo.log").display().to_string());
    // mihomo.log 属主修复（同 restart_script：root 属主 644 致用户侧截断静默失败）
    let user_q = shell_quote(&std::env::var("USER").unwrap_or_default());
    let chown_log = chown_log_fragment(&user_q, &log_q);

    // 第三层：只杀自己的 mihomo（不碰别的实例）+ bootstrap 后 controller 健康检查 → 同 session 回收
    // 自身 mihomo 按 binary 完整路径匹配（含 bundle-id 数据目录，全局唯一）。
    // dev/prod 默认端口隔离（dev +1），install 时互不影响。
    // **条件 kill**：用 flag 变量（any）替代 `[ -n "$pids" ]`——`do shell script` 外层是
    // AppleScript 双引号字符串，内部的双引号会提前终止它（-2740 编译错误）。flag 方式零双引号，
    // AppleScript 安全。有匹配 pid 才 sleep 1 等 TERM 生效再 KILL 兜底；首次安装无旧进程时跳过等待。
    // 健康检查用 curl 轮询 controller /version（secret 匹配）——mihomo 绑定失败不退出（降级运行），
    // pgrep 误判成功；只有 controller API 能确认 mihomo 真正可用。轮询替代固定 sleep：mihomo 实际
    // ~14ms ready，固定 sleep 白等；轮询 0.2s 间隔首次成功即 break，典型 <1s。
    // **bootstrap 前截断 mihomo.log**——launchd StandardOutPath 是 append 模式（不截断），
    // 旧失败尝试的 error 日志永远留在文件尾部。截断后 diagnose_launch_failure 只读到本次启动的日志，
    // 避免陈旧的 "address already in use" 导致误报。
    let matcher = format!(
        "ps -eo pid,args | grep -F {bin_q} | grep -F ' -d ' | grep -v grep | awk '{{print $1}}'"
    );
    let auth_header = shell_quote(&format!("Authorization: Bearer {}", params.secret));
    let ctrl_port = params.controller_port;
    let cmd = format!(
        "pids=$({matcher}); \
         any=0; \
         for p in $pids; do kill $p 2>/dev/null; any=1; done; \
         if [ $any -eq 1 ]; then \
            sleep 1; \
            for p in $pids; do kill -9 $p 2>/dev/null; done; \
         fi; \
         launchctl bootout system/{label} 2>/dev/null; \
         : > {log_q}; \
         {chown_log}; \
         cat {tmp_q} > {dest_q}; \
         chown root:wheel {dest_q}; \
         chmod 644 {dest_q}; \
         launchctl bootstrap system {dest_q}; \
         ok=0; \
         for i in 1 2 3 4 5 6 7 8 9 10; do \
            if curl -sf -m 1 -H {auth_header} http://127.0.0.1:{ctrl_port}/version >/dev/null 2>&1; then \
                ok=1; break; \
            fi; \
            sleep 0.2; \
         done; \
         if [ $ok -eq 0 ]; then \
            launchctl bootout system/{label} 2>/dev/null; \
            rm -f {dest_q}; \
            echo MIHOMO_LAUNCH_FAILED; \
         fi"
    );
    let script = elevate::do_shell_script(&cmd);
    let stdout = elevate::run_admin_shell(app, &script)
        .await
        .map_err(ElevateError::into_message)?;

    // 第三层：脚本检测到 fatal 已回收 plist，返回精确诊断
    if stdout.contains("MIHOMO_LAUNCH_FAILED") {
        return Err(diagnose_launch_failure(app, params));
    }

    // install 内的 curl 在 osascript root shell 中验证（独立进程上下文），但从 Voidnix 进程
    // 再次确认 controller 稳定——mihomo 刚 bootstrap 后 providers/geo 初始化有短暂抖动窗口，
    // root shell 的 curl 命中不代表 Voidnix 进程的 reqwest 首次连接必达。wait_ready 用同一
    // CONTROLLER client 轮询，成功即本进程连接已就绪，为紧随的 reload_config 铺路。
    let base = format!("http://127.0.0.1:{}", params.controller_port);
    match super::controller::wait_ready(&base, &params.secret, 8000).await {
        Ok(()) => Ok(()),
        Err(_) => Err(diagnose_launch_failure(app, params)),
    }
}

/// 卸载 LaunchDaemon（osascript 提权）：bootout 停 mihomo + 删 plist。core 升级/卸载用。
pub async fn uninstall_launchdaemon(app: &AppHandle) -> Result<(), String> {
    let label = daemon_label(app);
    let dest_q = shell_quote(&plist_install_path(&label).display().to_string());
    let cmd = format!("launchctl bootout system/{label} 2>/dev/null; rm -f {dest_q}");
    let script = elevate::do_shell_script(&cmd);
    elevate::run_admin_shell(app, &script)
        .await
        .map_err(ElevateError::into_message)?;
    Ok(())
}

/// 重启 LaunchDaemon 托管的 mihomo（osascript 提权）：`launchctl kickstart -k` 原子
/// kill + 立即拉起（绕过 ThrottleInterval）。核心假死 / controller 失联时的硬重启入口。
///
/// 顺带修复 mihomo.log 属主（历史安装中 launchd 以 root 创建，644 下用户进程截断静默
/// 失败，日志无限膨胀）：chown 为当前用户后，健康监测/stop 的用户侧截断恒可行。chown
/// 在 kickstart 前——launchd 每次 spawn 重新 open 路径，打开既有文件不改属主。
pub async fn restart_launchdaemon(app: &AppHandle) -> Result<(), String> {
    let label = daemon_label(app);
    let cmd = restart_script(
        &label,
        &plist_install_path(&label).display().to_string(),
        &ext_data_dir(app, "proxy")
            .map(|d| d.join("mihomo.log").display().to_string())
            .unwrap_or_default(),
        &std::env::var("USER").unwrap_or_default(),
    );
    let script = elevate::do_shell_script(&cmd);
    elevate::run_admin_shell(app, &script)
        .await
        .map_err(ElevateError::into_message)?;
    Ok(())
}

/// 提权脚本内 mihomo.log 属主修复片段（install/restart 共用）：空 USER（env 缺失）经
/// `[ -n ]` 守卫跳过，chown 失败静默（2>/dev/null）不阻断主流程。
fn chown_log_fragment(user_q: &str, log_q: &str) -> String {
    format!("if [ -n {user_q} ]; then chown {user_q} {log_q} 2>/dev/null; fi")
}

/// restart 提权脚本（纯函数，便于单测）：chown 修复日志属主（见 restart_launchdaemon）+
/// kickstart 原子 kill/拉起。**kickstart 失败回退 bootstrap**：restart_core 的门槛
/// （plist_installed）只查文件存在，plist 在而 job 未在 launchd（手动 `launchctl bootout` /
/// 上次 install 健康检查回收后 rm 失败等合法状态）时 kickstart 报 "Could not find
/// service"，直接从已装 plist bootstrap 拉起。`||` 短路：kickstart 成功整体即成功，
/// 两者皆败透传 bootstrap 的错误语义。
fn restart_script(label: &str, plist: &str, log: &str, user: &str) -> String {
    format!(
        "{}; launchctl kickstart -k system/{} 2>/dev/null || launchctl bootstrap system {}",
        chown_log_fragment(&shell_quote(user), &shell_quote(log)),
        label,
        shell_quote(plist),
    )
}

// ── 冲突诊断 ──

/// 查端口 LISTEN 占用者，返回 (pid, 完整 command 行)。无占用返回 None。
/// macOS 上 lsof 查网络端口无需 root，普通权限即可见所有进程的监听 socket。
/// 返回完整 command（非 basename）供 `is_own_mihomo` 做路径匹配；展示用
/// `process_name` 从 command 提取 basename。
fn port_occupant(port: u16) -> Option<(u32, String)> {
    let filter = format!("-iTCP:{port}");
    let out = Command::new("lsof")
        .args(["-nP", &filter, "-sTCP:LISTEN", "-t"])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let pid_str = stdout.lines().next()?.trim();
    if pid_str.is_empty() {
        return None;
    }
    let pid: u32 = pid_str.parse().ok()?;
    let cmd_out = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
        .ok()?;
    let command = String::from_utf8_lossy(&cmd_out.stdout).trim().to_string();
    if command.is_empty() {
        return None;
    }
    Some((pid, command))
}

/// command 是否是 Voidnix 自己的 mihomo（路径含 proxy 数据目录的 mihomo binary）。
fn is_own_mihomo(command: &str, app: &AppHandle) -> bool {
    let Ok(dir) = ext_data_dir(app, "proxy") else {
        return false;
    };
    command.contains(&dir.join("mihomo").display().to_string())
}

/// 从 command 行提取进程可读名（取程序路径 basename，如 Clash Verge 的 mihomo → mihomo）。
fn process_name(command: &str) -> String {
    command
        .split_whitespace()
        .next()
        .and_then(|s| {
            std::path::Path::new(s)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| command.to_string())
}

/// mihomo 启动失败诊断：读 mihomo.log 尾部识别端口/TUN 冲突 + lsof 查端口占用者。
/// 用于 install fatal 回收后 / wait_ready 超时后，把笼统的「启动失败」转为可操作的提示。
fn diagnose_launch_failure(app: &AppHandle, params: &super::core::RunParams) -> String {
    let mut reasons: Vec<String> = Vec::new();

    // 端口被别的程序占（install 前探测放行后，启动期间又被抢 / TUN 冲突以外的端口冲突）
    for &port in &[params.mixed_port, params.controller_port] {
        if let Some((pid, command)) = port_occupant(port) {
            if !is_own_mihomo(&command, app) {
                reasons.push(format!(
                    "{port} 端口被 {}（PID {pid}）占用",
                    process_name(&command)
                ));
            }
        }
    }

    // 读 mihomo.log 尾部识别已知错误模式（尾读窗口防无限增长日志的全量读取）
    if let Ok(dir) = ext_data_dir(app, "proxy") {
        let lines = super::lifecycle::read_log_tail(&dir.join("mihomo.log"), 0);
        for line in lines.iter().rev().take(30) {
            let l = line.to_lowercase();
            if l.contains("address already in use") {
                if !reasons.iter().any(|r| r.contains("端口")) {
                    reasons.push("监听端口已被占用".into());
                }
                break;
            }
            if (l.contains("tun") || l.contains("route"))
                && (l.contains("file exists") || l.contains("exists") || l.contains("permission"))
            {
                reasons.push("TUN 网卡或路由被其他代理工具占用".into());
                break;
            }
        }
    }

    if reasons.is_empty() {
        "代理核心启动失败，请查看 mihomo.log".into()
    } else {
        format!("代理核心启动失败：{}", reasons.join("；"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_script_applescript_safe_and_falls_back_to_bootstrap() {
        let cmd = restart_script(
            "com.litiantao.voidnix.mihomo",
            "/Library/LaunchDaemons/com.litiantao.voidnix.mihomo.plist",
            "/Users/foo/Library/Application Support/com.litiantao.voidnix/extensions/proxy/mihomo.log",
            "litiantao",
        );
        // 属主修复在前（kickstart respawn 前生效）
        assert!(cmd.starts_with("if [ -n 'litiantao' ]; then chown"));
        // kickstart 失败回退 bootstrap（plist 在而 job 未 bootstrapped 的合法状态）
        assert!(cmd.contains(
            "launchctl kickstart -k system/com.litiantao.voidnix.mihomo 2>/dev/null || launchctl bootstrap system '/Library/LaunchDaemons/com.litiantao.voidnix.mihomo.plist'"
        ));
        // do shell script 外层是 AppleScript 双引号字符串，内部双引号会提前终止它（-2740）
        assert!(!cmd.contains('"'), "脚本内不得出现双引号");
    }

    #[test]
    fn chown_log_fragment_guards_empty_user() {
        // 空 USER（env 缺失）：`[ -n '' ]` 守卫跳过，不产生 chown 报错
        assert_eq!(
            chown_log_fragment(&shell_quote(""), &shell_quote("/tmp/x")),
            "if [ -n '' ]; then chown '' '/tmp/x' 2>/dev/null; fi"
        );
    }
}
