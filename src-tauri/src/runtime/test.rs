use std::sync::atomic::{AtomicBool, Ordering};

/// 一次性标志：整个进程生命周期内自测只触发一次。
/// WebContent 内存超 350M 时 hide_window 会 navigate 重载 WKWebView，
/// 导致 main.ts 重新执行并再次调用 is_self_test_mode——环境变量是进程级的
/// 不会随页面重载消失，若无守卫自测会二次触发，与外部 CGEvent 测试脚本并发
/// 争抢同一 Vue store / 窗口状态，导致第 2 轮起 UI 乱跳。
static SELF_TEST_FIRED: AtomicBool = AtomicBool::new(false);

/// 自测模式判定：环境变量 VOIDNIX_SELF_TEST=1 时返回 true（**一次性**）。
/// 首次返回 true 后置位标志，后续调用（navigate 重载后 main.ts 再执行）一律 false。
#[tauri::command]
pub fn is_self_test_mode() -> bool {
    if std::env::var("VOIDNIX_SELF_TEST").as_deref() != Ok("1") {
        return false;
    }
    !SELF_TEST_FIRED.swap(true, Ordering::SeqCst)
}

/// 自测诊断日志：前端调用此命令将进度写到 stderr（WKWebView console 不可见于终端）。
/// 仅自测模式可用，防止生产环境被误用做日志通道。
#[tauri::command]
pub fn self_test_diag(message: String) {
    if std::env::var("VOIDNIX_SELF_TEST").as_deref() == Ok("1") {
        eprintln!("[self-test] {message}");
    }
}
