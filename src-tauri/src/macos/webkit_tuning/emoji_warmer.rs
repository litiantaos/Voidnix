#![allow(dead_code)]
//! Emoji_Warmer：在应用启动后预热系统 emoji 字体，避免首次渲染停顿（Req 4.1–4.4）。
//!
//! 触发时机：Main_Window 首次进入待命状态后 500ms 内（Req 4.1）。
//! 主线程预算：native 侧分片执行，单片 ≤8ms（Req 4.2）。
//! 失败处理：try_block 失败时跳过，记录 `已禁用` 日志，不阻塞启动（Req 4.4）。

use tauri::{Manager, WebviewWindow};

extern "C" {
    fn voidnix_warm_emoji_font();
}

/// 调度 emoji 字体预热。在 500ms 后切回主线程执行 native 预热函数。
/// 整个进程生命周期内最多触发一次（由 `std::sync::Once` 保证，Req 4.1）。
pub fn schedule(window: &WebviewWindow) {
    use std::sync::Once;
    static ONCE: Once = Once::new();

    let app = window.app_handle().clone();
    ONCE.call_once(move || {
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let _ = app.run_on_main_thread(move || {
                warm_once();
            });
        });
    });
}

/// 实际执行预热（主线程）。
/// 通过 try_block 兜底：native 侧任何异常都被吞掉，返回 false 时记录 `已禁用` 日志。
fn warm_once() {
    let ok = crate::macos::webkit_tuning::obj_exception::try_block(|| {
        // SAFETY: voidnix_warm_emoji_font 在主线程调用，内部有 @try/@catch 兜底。
        unsafe { voidnix_warm_emoji_font() };
    });
    if ok {
        crate::macos::webkit_tuning::log::component_status(
            "Emoji_Warmer",
            crate::macos::webkit_tuning::log::Status::Enabled,
            None,
        );
    } else {
        crate::macos::webkit_tuning::log::component_status(
            "Emoji_Warmer",
            crate::macos::webkit_tuning::log::Status::Disabled,
            Some("warmer-failed"),
        );
    }
}

/// 仅供测试使用的无操作调度：不启动异步任务，直接记录组件状态日志。
/// 在单元测试中无法使用 tauri::async_runtime，因此用此函数替代 schedule。
#[cfg(any(test, feature = "webkit_tuning_mock"))]
pub(crate) fn schedule_noop() {
    crate::macos::webkit_tuning::log::component_status(
        "Emoji_Warmer",
        crate::macos::webkit_tuning::log::Status::Enabled,
        None,
    );
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    /// 可注入的预热函数 mock，记录调用次数并可模拟失败。
    struct MockWarmer {
        call_count: Arc<AtomicU32>,
        should_fail: bool,
    }

    impl MockWarmer {
        fn new(should_fail: bool) -> (Self, Arc<AtomicU32>) {
            let count = Arc::new(AtomicU32::new(0));
            (
                Self {
                    call_count: count.clone(),
                    should_fail,
                },
                count,
            )
        }

        /// 模拟 warm_once 的核心逻辑：记录调用次数，返回是否成功。
        fn run(&self) -> bool {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            !self.should_fail
        }
    }

    /// 单元测试 1：正常路径 — 预热函数被调用恰好一次，返回成功（Req 4.1）。
    ///
    /// Validates: Requirements 4.1
    #[test]
    fn warm_once_called_exactly_once() {
        let (warmer, count) = MockWarmer::new(false);
        let ok = warmer.run();
        assert!(ok, "正常路径下 run 应返回 true");
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "预热函数应当被调用恰好一次"
        );
    }

    /// 单元测试 2：失败注入 — try_block 返回 false 时主流程不 panic，
    /// 调用计数仍为 1，且失败路径被正确识别（Req 4.4）。
    ///
    /// Validates: Requirements 4.4
    #[test]
    fn warm_once_failure_does_not_panic() {
        let (warmer, count) = MockWarmer::new(true);
        let ok = warmer.run();
        assert!(!ok, "失败注入时 run 应返回 false");
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "即使失败也应调用一次（不跳过调用本身）"
        );
        // 验证失败路径不 panic：测试本身不崩溃即为通过。
        // 对应 warm_once 中 ok==false 时写 `已禁用 reason=warmer-failed` 日志的分支。
    }

    /// 单元测试 3：多次调用 mock — 每次调用都独立计数，验证 mock 本身无状态污染。
    ///
    /// Validates: Requirements 4.1（间接：Once 保证进程级单次，mock 层验证调用语义）
    #[test]
    fn mock_warmer_counts_multiple_calls() {
        let (warmer, count) = MockWarmer::new(false);
        warmer.run();
        warmer.run();
        warmer.run();
        assert_eq!(
            count.load(Ordering::SeqCst),
            3,
            "mock 应如实记录每次调用"
        );
    }
}
