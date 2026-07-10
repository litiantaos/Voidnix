use objc2_app_kit::{NSApp, NSApplicationActivationOptions, NSWorkspace};
use objc2_foundation::{MainThreadMarker, NSURL};
use std::sync::atomic::{AtomicI32, Ordering};

/// 前台 PID 唯一源：显示主窗口前记录原前台 app PID，隐藏时恢复。
static PREV_FRONT_PID: AtomicI32 = AtomicI32::new(0);

/// 将 Voidnix 设为 active app（收 mouseMoved / 停下层 app hover）。
///
/// NonactivatingPanel 本身不会因 makeKey 激活 NSApp；show 路径必须显式调用，
/// 否则 macOS 26 上 mouse tracking 仍留在原前台 app，整窗穿透 hover。
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
/// 内还有 key window（主 panel、NSOpenPanel 等),就视为焦点在我们这里。
/// panel 丢 key 后,若前台已切到系统进程（授权弹窗、keychain 对话框等），
/// 同样视为交互流未中断,不触发 hide。
pub fn is_app_active() -> bool {
    if let Some(mtm) = MainThreadMarker::new() {
        if NSApp(mtm).keyWindow().is_some() {
            return true;
        }
        return is_system_frontmost();
    }
    false
}

/// frontmost app 是否为系统进程（bundle 路径以 `/System/` 开头）。
/// 用于检测系统级焦点接管（授权弹窗、keychain 对话框等），全局通用。
fn is_system_frontmost() -> bool {
    let ws = NSWorkspace::sharedWorkspace();
    let Some(app) = ws.frontmostApplication() else {
        return false;
    };
    let Some(url) = app.bundleURL() else {
        return false;
    };
    let path = NSURL::path(&url);
    path.map(|p| p.to_string().starts_with("/System/"))
        .unwrap_or(false)
}

/// 返回当前 frontmost app 的 PID（不含 Voidnix 自己）。
pub fn current_frontmost_pid() -> Option<i32> {
    let ws = NSWorkspace::sharedWorkspace();
    let pid = ws.frontmostApplication().map(|a| a.processIdentifier())?;
    let self_pid = std::process::id() as i32;
    if pid == self_pid {
        None
    } else {
        Some(pid)
    }
}

/// 把 key window 状态/前台 app 切换回指定 PID 进程，
/// 用于 Voidnix 隐藏时把焦点（含输入法 first responder）原样交还给原应用。
pub fn activate_app_by_pid(pid: i32) {
    if pid <= 0 {
        return;
    }
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

/// 记录当前前台 app PID（排除自身），存入唯一源。
pub fn capture_frontmost() -> i32 {
    let pid = current_frontmost_pid().unwrap_or(0);
    PREV_FRONT_PID.store(pid, Ordering::SeqCst);
    pid
}

/// 存入显式 PID 到唯一源（调用方已持有 pid，避免重复查询 frontmost）。
pub fn capture_pid(pid: i32) {
    PREV_FRONT_PID.store(pid, Ordering::SeqCst);
}

/// 读取唯一源中的 PID（不消费）。
pub fn captured_pid() -> i32 {
    PREV_FRONT_PID.load(Ordering::SeqCst)
}

/// 取出并清零唯一源（不 activate）。
/// 供需自行控制 activate 时机的调用方；常规还原请用 `restore_captured`。
pub fn take_captured_pid() -> i32 {
    PREV_FRONT_PID.swap(0, Ordering::SeqCst)
}

/// 从唯一源恢复前台 app：先 deactivate self，再 activate 原 app。
///
/// 若当前 frontmost 已是第三方（系统授权弹窗、用户点击其他应用窗口等），
/// 说明系统已主动转移焦点，遵循该归属不抢回 —— 否则 activate 会夺走系统弹窗焦点。
pub fn restore_captured() {
    deactivate_app();
    let pid = PREV_FRONT_PID.swap(0, Ordering::SeqCst);
    if pid > 0 {
        if let Some(front) = current_frontmost_pid() {
            if front != pid {
                return;
            }
        }
        activate_app_by_pid(pid);
    }
}
