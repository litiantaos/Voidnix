/// 自测模式判定：环境变量 VOIDNIX_SELF_TEST=1 时返回 true。
/// 前端 main.ts 在扩展 setup 完成后调用此命令，true 则启动自测流程，
/// 结果经 plugin-store 写到 config/test-report.json 供外部编排器读取。
#[tauri::command]
pub fn is_self_test_mode() -> bool {
    std::env::var("VOIDNIX_SELF_TEST").as_deref() == Ok("1")
}

/// 自测诊断日志：前端调用此命令将进度写到 stderr（WKWebView console 不可见于终端）。
/// 仅自测模式可用，防止生产环境被误用做日志通道。
#[tauri::command]
pub fn self_test_diag(message: String) {
    if std::env::var("VOIDNIX_SELF_TEST").as_deref() == Ok("1") {
        eprintln!("[self-test] {message}");
    }
}
