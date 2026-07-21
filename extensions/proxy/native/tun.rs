use crate::runtime::storage::ext_data_dir;
use std::process::Command;
use tauri::AppHandle;

/// 单引号 shell 转义：app data dir 含空格（Application Support），须引号包裹。
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// 提权启动 root mihomo（TUN 需 root 创建虚拟网卡 + auto-route）。
///
/// 先清理**所有 mihomo 进程**（Voidnix 自己的 + 其他 app 如 Clash Verge / Mihomo Party
/// 残留的），再启动新的。TUN 是系统独占资源（虚拟网卡 + 路由 `1.0.0.0/8` 等），两个
/// mihomo 实例不能同时占 TUN（`add route: file exists`）。用户在多个代理软件间切换时，
/// 其他 app 的 mihomo 可能残留（异常退出 / 用户未显式关闭），统一清理后启新，避免 TUN
/// 冲突致本进程启动失败。
///
/// 进程匹配模式：args 中含 `/mihomo`（binary 名）+ ` -d `（daemon 标志），覆盖各 app 的
/// mihomo 启动方式（`<bin> -d <config_dir>`），避免误杀同名进程；`grep -v grep` 排除
/// 当前 osascript 的 sh 包装（其 args 含 'grep' 字符串）。
///
/// 单次 osascript 提权完成「杀旧 + 启新」，免多次密码框。SIGTERM → 轮询确认退出
/// （至多 2s，撤 TUN/路由）→ SIGKILL 兜底 → sleep 1（端口/utun 释放）→ 启新。
///
/// SIGTERM 阶段用轮询而非固定 sleep：正常退出（几百 ms）更快进入下一阶段，异常给满 2s
/// 再强杀，避免 mihomo 未及撤销 utun 就被 SIGKILL 致残留虚拟网卡挡道新进程 TUN 创建。
///
/// `do shell script "... & echo $! > pidfile" with administrator privileges`：
/// mihomo 后台 detach（PPID→1），$! 写入 pidfile 供热重启/停止用。弹出系统授权对话框。
/// 输出重定向到 mihomo.log（启动失败时可查日志诊断）。
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
    // 杀所有 mihomo daemon（不区分 app），统一接管 TUN。matcher 单独 extract 便于 SIGTERM/
    // 轮询/SIGKILL 三阶段复用，避免重复书写长 pipeline 致脚本难读。
    let matcher = "ps -eo pid,args | grep -E '/mihomo( |$)' | grep -F ' -d ' | grep -v grep";
    let cmd = format!(
        "for p in $({matcher} | awk '{{print $1}}'); do \
            kill $p 2>/dev/null; \
         done; \
         i=0; \
         while {matcher} | grep -q .; do \
            i=$((i+1)); \
            if [ $i -gt 10 ]; then break; fi; \
            sleep 0.2; \
         done; \
         for p in $({matcher} | awk '{{print $1}}'); do \
            kill -9 $p 2>/dev/null; \
         done; \
         sleep 1; \
         rm -f {pidfile_q}; \
         {bin_q} -d {dir_q} >{logfile_q} 2>&1 & echo $! > {pidfile_q}"
    );
    let script = format!("do shell script \"{cmd}\" with administrator privileges");
    run_osascript(app, &script)
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
    run_osascript(app, &script)?;
    let _ = std::fs::remove_file(&pidfile);
    Ok(())
}

/// 执行 osascript，识别用户取消（-128）。
///
/// 调用期间暂停 click-outside 检测 + 置位 OSASCRIPT_RUNNING：`with administrator
/// privileges` 弹出的 SecurityAgent 授权框在主窗口外，用户点击（输密码/确认）会被判为
/// click-outside 触发 hideWindow；授权完成后 SecurityAgent 关闭，shell 命令仍跑 2-3s
/// 期间 frontmost 已还给原 app，is_app_active 会返 false 触发 blur hide——两类都会
/// 导致窗口被意外关闭。置位 OSASCRIPT_RUNNING 让 is_app_active 期间返 true 抑制 blur hide。
///
/// 收尾在主线程：make_key 恢复 panel 焦点（用户输完密码大概率想继续操作面板），再清 flag。
/// 与 is_app_active 的 run_on_main_thread 同线程串行，无竞态：key 未恢复前 is_app_active
/// 仍走 OSASCRIPT_RUNNING 分支返 true，blur hide 持续被抑制。
fn run_osascript(app: &AppHandle, script: &str) -> Result<(), String> {
    crate::platform::click_monitor::suppress(true);
    crate::platform::focus::set_osascript_running(true);
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
    // 主线程收尾：make_key 恢复焦点（panel 可见时）+ 清 flag
    let app_clone = app.clone();
    let scheduled = app.run_on_main_thread(move || {
        use tauri::Manager;
        if let Some(window) = app_clone.get_webview_window("main") {
            // 镜像 frontmost_watcher 的可见性判定：hide 不 orderOut，alpha=0 视为已隐藏
            let visible = window
                .ns_window()
                .ok()
                .and_then(|p| {
                    let raw = p.cast::<objc2_app_kit::NSWindow>();
                    unsafe { raw.as_ref().map(|ns| ns.alphaValue() >= 0.01) }
                })
                .unwrap_or(false);
            if visible {
                crate::platform::window::make_key_window(&window);
            }
        }
        crate::platform::focus::set_osascript_running(false);
    });
    if scheduled.is_err() {
        // 调度失败（app 退出等极端情况）兜底直接清 flag，避免泄漏
        crate::platform::focus::set_osascript_running(false);
    }
    crate::platform::click_monitor::suppress(false);
    result
}
