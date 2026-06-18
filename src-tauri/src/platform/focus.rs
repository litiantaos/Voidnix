use objc2_app_kit::{NSApp, NSApplicationActivationOptions, NSWorkspace};
use objc2_foundation::MainThreadMarker;

pub fn activate_app() {
    if let Some(mtm) = MainThreadMarker::new() {
        let app = NSApp(mtm);
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
    }
}

/// 触发系统重新评估 key window 归属。
///
/// NonactivatingPanel + LSUIElement 模式下,NSApp 始终 inactive,deactivate
/// 表面是 no-op,但仍会广播 `NSApplicationWillResignActiveNotification` —— 配合
/// 后续 `activate_app_by_pid(prev_pid)` 把 system key / first responder 还给
/// 原应用,缺这一步 macOS 会跳过对已 frontmost app 的 activate 请求,
/// 用户得手动点输入框才能继续打字。
pub fn deactivate_app() {
    if let Some(mtm) = MainThreadMarker::new() {
        NSApp(mtm).deactivate();
    }
}

/// 判断键盘焦点是否仍在 Voidnix 应用内。
///
/// NonactivatingPanel + LSUIElement 模式下,NSApp 始终 inactive,
/// `isActive` 不再能反映焦点位置 —— 改用 `keyWindow` 判断：只要 NSApp
/// 内还有 key window（主 panel、NSOpenPanel 等),就视为焦点在我们这里,
/// 反之表示用户已切到其他应用,前端应主动 hide。
pub fn is_app_active() -> bool {
    if let Some(mtm) = MainThreadMarker::new() {
        return NSApp(mtm).keyWindow().is_some();
    }
    false
}

/// 返回当前 frontmost app 的 PID（不含 Voidnix 自己）。
pub fn current_frontmost_pid() -> Option<i32> {
    let ws = NSWorkspace::sharedWorkspace();
    let pid = ws.frontmostApplication().map(|a| a.processIdentifier())?;
    let self_pid = std::process::id() as i32;
    if pid == self_pid { None } else { Some(pid) }
}

/// 把 key window 状态/前台 app 切换回指定 PID 进程，
/// 用于 Voidnix 隐藏时把焦点（含输入法 first responder）原样交还给原应用。
pub fn activate_app_by_pid(pid: i32) {
    if pid <= 0 { return; }
    let ws = NSWorkspace::sharedWorkspace();
    if let Some(target) = ws
        .runningApplications()
        .iter()
        .find(|a| a.processIdentifier() == pid)
    {
        #[allow(deprecated)]
        target.activateWithOptions(NSApplicationActivationOptions::ActivateIgnoringOtherApps);
    }
}
