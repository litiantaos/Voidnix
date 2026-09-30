//! osascript 提权原语：`do shell script ... with administrator privileges` 的
//! 脚本拼装与安全执行器。proxy（LaunchDaemon 安装/重启/卸载）与 awake（睡眠
//! daemon 安装）共用。

use std::process::Command;
use tauri::{AppHandle, Manager};

/// 提权执行失败分类。
pub enum ElevateError {
    /// 用户取消了管理员授权弹窗（osascript error -128）
    Cancelled,
    /// 授权通过但命令执行失败，携带 stderr
    Failed(String),
}

impl ElevateError {
    /// 展开为面向用户的错误文案（proxy 的历史文案语义，含取消识别）
    pub fn into_message(self) -> String {
        match self {
            ElevateError::Cancelled => "已取消授权".into(),
            ElevateError::Failed(msg) => msg,
        }
    }
}

/// 单引号 shell 转义：app data dir 含空格（Application Support），须引号包裹。
/// 单引号内唯一无法出现的字符是单引号本身，以 `'\''` 断开重开转义。
pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// 包裹 shell 命令为 `do shell script` AppleScript 源，并做 AppleScript 字符串转义。
/// cmd 须先经 shell_quote 拼装；其 `'\''` 转义携带反斜杠、用户名/路径含引号或反斜杠时，
/// 未转义嵌入双引号 AppleScript 字符串会提前终止或编译失败（-2740），提权入口整体失效
/// （用户名含 `'` 的账户 restart/install 恒报错）。`\` 先于 `"` 转义，防止转义引入的
/// 反斜杠被二次转义。
pub fn do_shell_script(cmd: &str) -> String {
    format!(
        "do shell script \"{}\" with administrator privileges",
        cmd.replace('\\', "\\\\").replace('"', "\\\"")
    )
}

/// osascript 失败输出分类（纯函数供单测）：-128 / User canceled 为取消，其余透传 stderr。
fn classify_osascript_failure(stderr: &str, fallback: &str) -> ElevateError {
    let lower = stderr.to_lowercase();
    if stderr.contains("-128") || lower.contains("user canceled") {
        return ElevateError::Cancelled;
    }
    let message = stderr.trim();
    ElevateError::Failed(if message.is_empty() {
        fallback.into()
    } else {
        message.into()
    })
}

/// 经 osascript 管理员授权执行 shell 命令，返回 stdout。
///
/// 阻塞体经 `spawn_blocking` 执行——`with administrator privileges` 的 SecurityAgent 授权
/// 等待可达分钟级，同步阻塞会钉死一个 tokio worker（仅 4 个），期间其余异步任务挤占剩余。
///
/// 调用期间暂停 click-outside 检测 + 置位 OSASCRIPT_RUNNING：授权框在主窗口外，用户点击
/// （输密码/确认）会被判为 click-outside 触发 hideWindow；授权完成后 SecurityAgent 关闭，
/// shell 命令仍跑 2-3s 期间 frontmost 已还给原 app，is_app_active 会返 false 触发 blur
/// hide——两类都会导致窗口被意外关闭。置位 OSASCRIPT_RUNNING 让 is_app_active 期间返
/// true 抑制 blur hide。
///
/// 收尾在主线程：make_key 恢复 panel 焦点（用户输完密码大概率想继续操作），再清 flag。
/// 与 is_app_active 的 run_on_main_thread 同线程串行，无竞态：key 未恢复前 is_app_active
/// 仍走 OSASCRIPT_RUNNING 分支返 true，blur hide 持续被抑制。
pub async fn run_admin_shell(app: &AppHandle, cmd: &str) -> Result<String, ElevateError> {
    let script = do_shell_script(cmd);
    crate::platform::click_monitor::suppress(true);
    crate::platform::focus::set_osascript_running(true);
    let result = tokio::task::spawn_blocking(move || {
        let out = Command::new("osascript")
            .args(["-e", &script])
            .output()
            .map_err(|e| ElevateError::Failed(format!("osascript 调用失败: {e}")))?;
        if out.status.success() {
            return Ok(String::from_utf8_lossy(&out.stdout).trim().to_string());
        }
        Err(classify_osascript_failure(
            &String::from_utf8_lossy(&out.stderr),
            "osascript exited with error",
        ))
    })
    .await
    .unwrap_or_else(|e| Err(ElevateError::Failed(format!("osascript 执行失败: {e}"))));
    // 主线程收尾：make_key 恢复焦点（panel 可见时）+ 清 flag
    let app_clone = app.clone();
    let scheduled = app.run_on_main_thread(move || {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quotes_spaces_and_apostrophes() {
        assert_eq!(shell_quote("/a b/c"), "'/a b/c'");
        assert_eq!(shell_quote("/us'er/x"), "'/us'\\''er/x'");
    }

    #[test]
    fn do_shell_script_escapes_applescript_string() {
        // shell_quote 的 `'\''` 转义含反斜杠：未转义嵌入 AppleScript 双引号字符串会 -2740
        let quoted = shell_quote("o'brien");
        let script = do_shell_script(&format!("chown {quoted} /tmp/x"));
        // 反斜杠转义为 `\\`，AppleScript 解码后 shell 仍收到正确的 `'\''`
        assert!(script.contains("chown 'o'\\\\''brien' /tmp/x"));
        // 引号内的双引号同样转义（未来脚本若引入），外层仍是一对完整双引号
        let script2 = do_shell_script("echo \"hi\"");
        assert!(script2.contains("echo \\\"hi\\\""));
    }

    #[test]
    fn osascript_failure_classification() {
        // -128 / User canceled（大小写不敏感）→ 取消
        assert!(matches!(
            classify_osascript_failure("134:148: execution error: User canceled. (-128)", ""),
            ElevateError::Cancelled
        ));
        assert!(matches!(
            classify_osascript_failure("execution error: user canceled", ""),
            ElevateError::Cancelled
        ));
        // 其他 → Failed 透传 stderr；空 stderr 用 fallback
        assert!(matches!(
            classify_osascript_failure("some failure", ""),
            ElevateError::Failed(m) if m == "some failure"
        ));
        assert!(matches!(
            classify_osascript_failure("  ", "osascript exited with error"),
            ElevateError::Failed(m) if m == "osascript exited with error"
        ));
    }
}
