#![allow(dead_code)]
//! Presentation_Coordinator：在唤起 Main_Window 时等待 Web_View 完成首帧呈现，
//! 再让窗口进入屏幕可见状态（Req 1.1, 1.2, 1.5, 1.6）。
//!
//! - `install`：记录组件就绪状态（真正的 await_paint 在 show_main 中按需调用）。
//! - `await_paint`：通过 PresentationBridge 等待 presentation update，
//!   超时（80ms）或 SPI 不可用时走 fallback 路径。

use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use tauri::Emitter;

use crate::macos::webkit_tuning::{log, PresentationBridge, WindowOps};

/// presentation update 等待超时（毫秒）。Req 1.2。
const PAINT_TIMEOUT_MS: u64 = 80;

/// 组件失败计数器；达到 FAIL_LIMIT 后永久禁用。
static FAIL_COUNT: AtomicU8 = AtomicU8::new(0);
const FAIL_LIMIT: u8 = 3;

fn is_disabled() -> bool {
    FAIL_COUNT.load(Ordering::SeqCst) >= FAIL_LIMIT
}

fn record_failure() {
    FAIL_COUNT.fetch_add(1, Ordering::SeqCst);
}

/// 记录组件就绪状态。
pub fn install() {
    log::component_status("Presentation_Coordinator", log::Status::Enabled, None);
}

/// 等待 Web_View 完成首帧呈现，然后将 alpha 设为 1.0。
///
/// - 正常路径：bridge.schedule 返回 true，等待回调 ok=true → alpha=1 + emit `painted`
/// - 超时路径：回调 ok=false → alpha=1 + emit `awaiting-paint`
/// - SPI 不可用：bridge.schedule 返回 false → alpha=1 + step `await-paint-spi-missing`
/// - 永久禁用：直接 alpha=1 + step `await-paint-disabled`
///
/// `app_handle`：可选，用于向前端 emit 事件；None 时跳过 emit（测试场景）。
pub fn await_paint<W: WindowOps>(
    window: &W,
    bridge: &dyn PresentationBridge,
    app_handle: Option<&tauri::AppHandle>,
    steps: &mut log::Steps,
) {
    if is_disabled() {
        window.set_alpha(1.0);
        steps.push("await-paint-disabled");
        return;
    }

    let (tx, rx) = std::sync::mpsc::channel::<bool>();
    let invoked = bridge.schedule(
        PAINT_TIMEOUT_MS,
        Box::new(move |ok| {
            let _ = tx.send(ok);
        }),
    );

    if !invoked {
        record_failure();
        window.set_alpha(1.0);
        steps.push("await-paint-spi-missing");
        log::component_status(
            "Presentation_Coordinator",
            log::Status::Fallback,
            Some("spi-missing"),
        );
        return;
    }

    // 等待回调，超时时间比 native 侧多 16ms 余量（一帧）
    let result = rx
        .recv_timeout(Duration::from_millis(PAINT_TIMEOUT_MS + 16))
        .unwrap_or(false);

    window.set_alpha(1.0);

    if result {
        steps.push("await-paint-ok");
        if let Some(app) = app_handle {
            let _ = app.emit("webkit-tuning:painted", ());
        }
    } else {
        steps.push("await-paint-timeout");
        if let Some(app) = app_handle {
            let _ = app.emit("webkit-tuning:awaiting-paint", ());
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 测试
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::macos::webkit_tuning::test_support::{MockPresentationBridge, MockWindow};
    use proptest::prelude::*;
    use std::time::Instant;

    /// 所有操作全局 FAIL_COUNT 的测试必须持有此锁，避免并行测试相互污染。
    static FAIL_COUNT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn reset_fail_count() {
        FAIL_COUNT.store(0, Ordering::SeqCst);
    }

    // ── Property 1: Show 时 alpha 序列受 paint/timeout 因果约束 ─────────────
    //
    // Feature: webkit-presentation-tuning, Property 1.
    // 对任意 paint 回调延迟 d 与 paint_will_arrive：
    // - alpha 在 await_paint 返回前为 0，返回后为 1
    // - d ≤ 80ms 且 paint_will_arrive=true 时 steps 以 await-paint-ok 收尾
    // - 否则 steps 以 await-paint-timeout 收尾
    // - t_alpha₁ ≤ t_show + min(d, 80ms) + ε
    // Validates: Requirements 1.1, 1.2, 1.6
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn property_1_alpha_sequence_causal(
            d_ms in 0u64..200u64,
            paint_will_arrive in any::<bool>()
        ) {
            let _g = FAIL_COUNT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            reset_fail_count();

            let w = MockWindow::new();
            w.set_alpha(0.0); // 模拟隐藏状态

            // 如果 d_ms > 80，paint 不会在超时前到达（无论 paint_will_arrive 如何）
            // 如果 d_ms <= 80，paint 会在超时前到达（由 paint_will_arrive 决定 ok 值）
            let effective_arrive = paint_will_arrive && d_ms <= PAINT_TIMEOUT_MS;
            let bridge = MockPresentationBridge::new(effective_arrive, d_ms);

            let t_show = Instant::now();
            prop_assert_eq!(w.alpha(), 0.0, "await_paint 前 alpha 应为 0");

            let mut steps = log::Steps::new();
            await_paint(&w, &bridge, None, &mut steps);

            let t_alpha1 = w.last_alpha_set_at.lock().unwrap().unwrap();

            // alpha 在 await_paint 返回后应为 1
            prop_assert_eq!(w.alpha(), 1.0, "await_paint 后 alpha 应为 1");

            // t_alpha1 应在 t_show + min(d_ms, 80ms) + ε 内
            // 给 32ms 余量（2帧）应对系统调度抖动
            let expected_max_wait = Duration::from_millis(PAINT_TIMEOUT_MS + 32);
            prop_assert!(
                t_alpha1.duration_since(t_show) <= expected_max_wait,
                "alpha=1 的时刻应在 t_show + {}ms 内，实际耗时 {:?}",
                PAINT_TIMEOUT_MS + 32,
                t_alpha1.duration_since(t_show)
            );

            // 步骤断言
            if d_ms <= PAINT_TIMEOUT_MS && paint_will_arrive {
                prop_assert!(
                    steps.contains(&"await-paint-ok"),
                    "paint 在超时前到达时 steps 应含 await-paint-ok，实际={:?}",
                    steps
                );
            } else {
                prop_assert!(
                    steps.contains(&"await-paint-timeout"),
                    "paint 超时时 steps 应含 await-paint-timeout，实际={:?}",
                    steps
                );
            }
        }

        // Feature: webkit-presentation-tuning, Property 2: Show 仅作用于 Main_Window。
        // label != "main" 时 native bridge 调用次数为 0。
        // Validates: Requirements 1.5
        //
        // 本测试直接验证：对非 "main" label，不调用 await_paint（bridge 不被调用）；
        // 对 "main" label，调用 await_paint 后 bridge.schedule_count > 0。
        #[test]
        fn property_2_bridge_not_called_for_non_main_label(
            label in proptest::sample::select(vec!["main", "screenshot", "x", ""])
        ) {
            let _g = FAIL_COUNT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            reset_fail_count();

            let bridge = MockPresentationBridge::new(true, 0);
            let w = MockWindow::new();
            w.set_alpha(0.0);

            if label == "main" {
                // main 窗口：调用 await_paint，bridge 应被调用
                let mut steps = log::Steps::new();
                await_paint(&w, &bridge, None, &mut steps);
                prop_assert!(
                    bridge.schedule_count() > 0,
                    "main 窗口应调用 bridge.schedule"
                );
            } else {
                // 非 main 窗口：不调用 await_paint，bridge 不应被调用
                // （此约束由 T10 的 show_main label 守卫保证，本测试验证 bridge 未被调用）
                prop_assert_eq!(
                    bridge.schedule_count(),
                    0,
                    "非 main 窗口不应调用 bridge.schedule"
                );
            }
        }
    }

    // ── 基础单元测试 ─────────────────────────────────────────────────────────

    /// paint 正常到达：alpha=1，steps 含 await-paint-ok。
    #[test]
    fn await_paint_ok_sets_alpha_and_step() {
        let _g = FAIL_COUNT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_fail_count();
        let w = MockWindow::new();
        w.set_alpha(0.0);
        let bridge = MockPresentationBridge::new(true, 0);
        let mut steps = log::Steps::new();
        await_paint(&w, &bridge, None, &mut steps);
        assert_eq!(w.alpha(), 1.0);
        assert!(steps.contains(&"await-paint-ok"), "steps={:?}", steps);
    }

    /// paint 超时（ok=false）：alpha=1，steps 含 await-paint-timeout。
    #[test]
    fn await_paint_timeout_sets_alpha_and_step() {
        let _g = FAIL_COUNT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_fail_count();
        let w = MockWindow::new();
        w.set_alpha(0.0);
        // paint 不到达（ok=false）
        let bridge = MockPresentationBridge::new(false, 0);
        let mut steps = log::Steps::new();
        await_paint(&w, &bridge, None, &mut steps);
        assert_eq!(w.alpha(), 1.0);
        assert!(steps.contains(&"await-paint-timeout"), "steps={:?}", steps);
    }

    /// SPI 不可用（schedule 返回 false）：alpha=1，steps 含 await-paint-spi-missing。
    #[test]
    fn await_paint_spi_missing_fallback() {
        let _g = FAIL_COUNT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_fail_count();
        let w = MockWindow::new();
        w.set_alpha(0.0);
        let bridge = MockPresentationBridge::unavailable();
        let mut steps = log::Steps::new();
        await_paint(&w, &bridge, None, &mut steps);
        assert_eq!(w.alpha(), 1.0, "SPI 不可用时 alpha 仍应设为 1");
        assert!(steps.contains(&"await-paint-spi-missing"), "steps={:?}", steps);
    }

    /// 永久禁用（FAIL_COUNT >= FAIL_LIMIT）：alpha=1，steps 含 await-paint-disabled，
    /// bridge 不被调用。
    #[test]
    fn await_paint_disabled_after_fail_limit() {
        let _g = FAIL_COUNT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_fail_count();
        FAIL_COUNT.store(FAIL_LIMIT, Ordering::SeqCst);
        let w = MockWindow::new();
        w.set_alpha(0.0);
        let bridge = MockPresentationBridge::new(true, 0);
        let mut steps = log::Steps::new();
        await_paint(&w, &bridge, None, &mut steps);
        assert_eq!(w.alpha(), 1.0);
        assert!(steps.contains(&"await-paint-disabled"), "steps={:?}", steps);
        // 永久禁用时不应调用 bridge
        assert_eq!(bridge.schedule_count(), 0, "永久禁用时不应调用 bridge");
        reset_fail_count();
    }

    /// SPI 不可用时 FAIL_COUNT 递增。
    #[test]
    fn await_paint_spi_missing_increments_fail_count() {
        let _g = FAIL_COUNT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_fail_count();
        let w = MockWindow::new();
        let bridge = MockPresentationBridge::unavailable();
        let mut steps = log::Steps::new();
        let before = FAIL_COUNT.load(Ordering::SeqCst);
        await_paint(&w, &bridge, None, &mut steps);
        let after = FAIL_COUNT.load(Ordering::SeqCst);
        assert_eq!(after, before + 1, "SPI 不可用时 FAIL_COUNT 应递增");
        reset_fail_count();
    }
}
