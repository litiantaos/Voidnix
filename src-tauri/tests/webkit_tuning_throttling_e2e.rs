// T16：rAF/setTimeout 隐藏期间不被节流的集成测试。
//
// 此测试需要真实 WKWebView 环境（macOS 窗口服务器），无法在无头 CI 中自动化。
// 验收通过手工验收 checklist（manual-acceptance.md）中的验收项 5 覆盖。
//
// 如需在本地运行，请参考 manual-acceptance.md 中的执行步骤。
//
// 自动化验证方式（需要真实 Tauri 应用运行时）：
// 1. 启动 Voidnix dev binary
// 2. 唤起后立即触发 hide_window
// 3. 隐藏 5s 期间前端 requestAnimationFrame 计数 + 100ms 间隔的 30 次 setTimeout 漂移采样
// 4. 通过 invoke 'report_throttling_probe' 回报
// 5. 断言：rAFCount ≥ 150（≥30Hz × 5s）、maxDriftMs ≤ 50
//
// 对照组：VOIDNIX_DISABLE_WEBKIT_TUNING=1 下同测试用例预期失败（rAFCount < 150 或 maxDriftMs > 50）

#[test]
#[ignore = "需要真实 WKWebView 环境，通过手工验收 checklist 覆盖"]
fn throttling_raf_not_throttled_when_hidden() {
    // 占位：真实测试需要 Tauri 应用运行时
}
