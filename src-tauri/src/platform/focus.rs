use objc2_app_kit::{NSApp, NSApplicationActivationOptions, NSWorkspace};
use objc2_foundation::{MainThreadMarker, NSURL};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

/// 前台 PID 唯一源：显示主窗口前记录原前台 app PID，隐藏时恢复。
static PREV_FRONT_PID: AtomicI32 = AtomicI32::new(0);

/// osascript `with administrator privileges` 执行期间置位。SecurityAgent 接管 frontmost
/// 时 is_system_frontmost 已能识别；但用户输完密码后 SecurityAgent 先关闭，shell 命令
/// （kill mihomo + sleep + spawn）仍跑 2-3s，frontmost 已还给原 app（非系统进程），
/// 此时 is_app_active 返 false 会触发 blur hide 关窗——与「授权未完成窗口就关闭」同类。
/// 置位期间视为交互流未中断。tun.rs::run_osascript 进入时置位，主线程收尾时清零。
static OSASCRIPT_RUNNING: AtomicBool = AtomicBool::new(false);

/// 标记 osascript 授权是否执行中（由 tun.rs::run_osascript 调用）。
pub fn set_osascript_running(v: bool) {
    OSASCRIPT_RUNNING.store(v, Ordering::SeqCst);
}

/// 将 Voidnix 设为 active app。
///
/// 主窗 show **故意不调用**（保持 NonactivatingPanel 轻浮层，避免原 app
/// resign active 导致聚焦视图消失）。截图全屏 overlay 等需要独占鼠标/键盘
/// 的场景才显式 activate。
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
/// 第三道兜底：osascript 授权后续 shell 命令执行期间（SecurityAgent 已关，
/// frontmost 已还给原 app），仍视为交互流未中断——避免 blur hide 关窗。
pub fn is_app_active() -> bool {
    if let Some(mtm) = MainThreadMarker::new() {
        if NSApp(mtm).keyWindow().is_some() {
            return true;
        }
        if is_system_frontmost() {
            return true;
        }
        // frontmost 为 Voidnix 自身（WKWebView 聚焦可编辑元素触发的自我激活,
        // 激活事务可能短暂夺走 panel key）:交互流在自己身上,同样不触发 blur hide
        if frontmost_is_self() {
            return true;
        }
        if OSASCRIPT_RUNNING.load(Ordering::SeqCst) {
            return true;
        }
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

/// frontmost 是否为 Voidnix 自身（WKWebView 聚焦可编辑元素触发的自我激活瞬态）。
fn frontmost_is_self() -> bool {
    let ws = NSWorkspace::sharedWorkspace();
    ws.frontmostApplication()
        .map(|a| a.processIdentifier() == std::process::id() as i32)
        .unwrap_or(false)
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

/// 记录当前前台 app PID，存入唯一源。
///
/// frontmost 是 Voidnix 自身时**不覆盖**（保留上次记录）：WKWebView 在应用未激活时
/// 聚焦可编辑元素会激活本应用（textarea.focus() → activateIgnoringOtherApps），
/// 若此刻恰好被 capture，写入 0 会让 frontmost_watcher 把后续任何 app 激活都
/// 判为「用户主动切换」而 dismiss 窗口。自身 frontmost 是瞬态，跳过即可。
pub fn capture_frontmost() -> i32 {
    match current_frontmost_pid() {
        Some(pid) => {
            PREV_FRONT_PID.store(pid, Ordering::SeqCst);
            pid
        }
        None => PREV_FRONT_PID.load(Ordering::SeqCst),
    }
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
