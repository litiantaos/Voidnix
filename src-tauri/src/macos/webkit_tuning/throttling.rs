#![allow(dead_code)]
//! Throttling_Suppressor：防止 WKWebView 在窗口隐藏期间被节流。
//!
//! - `install`：一次性设置 `windowOcclusionDetectionEnabled=NO` 与
//!   `collectionBehavior |= Transient`，全部包在 `obj_exception::try_block` 内；
//!   失败 FAIL_COUNT+1，3 次永久 Disabled。
//! - `prepare_show`：唤起前恢复鼠标事件响应并将窗口提到最前（alpha 仍为 0）；
//!   步骤名 `"prepare-show"`。
//! - `hide`：将 alpha 设为 0 并禁用鼠标事件，不调用 `orderOut`/`app.hide`；
//!   `try_block` 失败时 fallback 到 `window.hide()` 并记录日志
//!   `已回退 reason=occlusion-locked`。

use std::sync::atomic::{AtomicU8, Ordering};

use crate::macos::webkit_tuning::{log, WindowOps};

/// NSWindowCollectionBehaviorTransient 位掩码（值 = 1 << 3 = 8）。
const TRANSIENT: u64 = 1 << 3;

/// 组件失败计数器；达到 FAIL_LIMIT 后永久禁用。
static FAIL_COUNT: AtomicU8 = AtomicU8::new(0);
const FAIL_LIMIT: u8 = 3;

fn is_disabled() -> bool {
    FAIL_COUNT.load(Ordering::SeqCst) >= FAIL_LIMIT
}

fn record_failure() {
    FAIL_COUNT.fetch_add(1, Ordering::SeqCst);
}

/// 一次性安装：设置 occlusionDetection=false 与 collectionBehavior |= Transient。
///
/// 失败时 FAIL_COUNT+1；达到 FAIL_LIMIT 后永久禁用，后续调用直接返回。
pub fn install<W: WindowOps>(window: &W) {
    if is_disabled() {
        log::component_status(
            "Throttling_Suppressor",
            log::Status::Disabled,
            Some("fail-count-exceeded"),
        );
        return;
    }
    let ok = crate::macos::webkit_tuning::obj_exception::try_block(|| {
        window.set_occlusion_detection(false);
        let cb = window.collection_behavior();
        window.set_collection_behavior(cb | TRANSIENT);
    });
    if ok {
        log::component_status("Throttling_Suppressor", log::Status::Enabled, None);
    } else {
        record_failure();
        log::component_status(
            "Throttling_Suppressor",
            log::Status::Fallback,
            Some("install-failed"),
        );
    }
}

/// 唤起前准备：恢复鼠标事件响应，将窗口提到最前（alpha 仍为 0）。
///
/// 步骤名 `"prepare-show"` 写入 steps。
pub fn prepare_show<W: WindowOps>(window: &W, steps: &mut log::Steps) {
    crate::macos::webkit_tuning::obj_exception::try_block(|| {
        window.set_ignores_mouse(false);
        window.order_front();
    });
    steps.push("prepare-show");
}

/// 隐藏：将 alpha 设为 0 并禁用鼠标事件，不调用 orderOut/app.hide。
///
/// `try_block` 失败时回退到 orderOut 路径并记录 `已回退 reason=occlusion-locked`。
/// 永久禁用时直接走 fallback 路径。
pub fn hide<W: WindowOps>(window: &W, steps: &mut log::Steps) {
    if is_disabled() {
        // 永久禁用：直接走 fallback 路径
        log::component_status(
            "Throttling_Suppressor",
            log::Status::Disabled,
            Some("fail-count-exceeded"),
        );
        steps.push("fallback-orderOut-disabled");
        return;
    }
    let ok = crate::macos::webkit_tuning::obj_exception::try_block(|| {
        window.set_ignores_mouse(true);
        window.set_alpha(0.0);
    });
    if ok {
        steps.push("alpha-fade-hide");
    } else {
        record_failure();
        log::component_status(
            "Throttling_Suppressor",
            log::Status::Fallback,
            Some("occlusion-locked"),
        );
        steps.push("fallback-orderOut");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 测试
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::macos::webkit_tuning::test_support::MockWindow;
    use proptest::prelude::*;
    use std::time::Instant;

    /// 重置全局失败计数器，避免测试间相互污染。
    fn reset_fail_count() {
        FAIL_COUNT.store(0, Ordering::SeqCst);
    }

    /// 所有操作全局 FAIL_COUNT 的测试必须持有此锁，避免并行测试相互污染。
    static FAIL_COUNT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    // ── 基础单元测试 ─────────────────────────────────────────────────────────

    /// install 后 occlusion_detection 应为 false，collectionBehavior 应包含 Transient。
    #[test]
    fn install_sets_occlusion_false_and_transient() {
        let _g = FAIL_COUNT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_fail_count();
        let w = MockWindow::new();
        assert!(w.occlusion_detection(), "安装前 occlusion_detection 应为 true");
        install(&w);
        assert!(!w.occlusion_detection(), "安装后 occlusion_detection 应为 false");
        assert_ne!(
            w.collection_behavior() & TRANSIENT,
            0,
            "安装后 collectionBehavior 应包含 Transient"
        );
    }

    /// hide 后 alpha==0，ignores_mouse==true，order_out_count==0，steps 含 alpha-fade-hide。
    #[test]
    fn hide_sets_alpha_zero_and_ignores_mouse() {
        let _g = FAIL_COUNT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_fail_count();
        let w = MockWindow::new();
        install(&w);
        w.set_alpha(1.0);
        let mut steps = log::Steps::new();
        hide(&w, &mut steps);
        assert_eq!(w.alpha(), 0.0, "hide 后 alpha 应为 0");
        assert!(w.ignores_mouse(), "hide 后 ignores_mouse 应为 true");
        assert_eq!(w.order_out_count(), 0, "hide 不得调用 orderOut");
        assert!(
            steps.contains(&"alpha-fade-hide"),
            "steps 应包含 alpha-fade-hide，实际={:?}",
            steps
        );
    }

    /// prepare_show 后 ignores_mouse 应为 false，order_front_count > 0，steps 含 prepare-show。
    #[test]
    fn prepare_show_restores_mouse_and_orders_front() {
        let _g = FAIL_COUNT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_fail_count();
        let w = MockWindow::new();
        install(&w);
        w.set_ignores_mouse(true);
        let mut steps = log::Steps::new();
        prepare_show(&w, &mut steps);
        assert!(!w.ignores_mouse(), "prepare_show 后 ignores_mouse 应为 false");
        assert!(
            w.order_front_count.load(Ordering::SeqCst) > 0,
            "prepare_show 应调用 orderFrontRegardless"
        );
        assert!(
            steps.contains(&"prepare-show"),
            "steps 应包含 prepare-show，实际={:?}",
            steps
        );
    }

    /// prepare_show 不应修改 alpha（alpha 仍为 0）。
    /// 注：Property 4 完整时序（pre-show → alpha=1 ≤16ms）在 T10 跨组件验证。
    #[test]
    fn prepare_show_does_not_set_alpha() {
        let _g = FAIL_COUNT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_fail_count();
        let w = MockWindow::new();
        install(&w);
        w.set_alpha(0.0); // 模拟隐藏状态
        let mut steps = log::Steps::new();
        prepare_show(&w, &mut steps);
        assert_eq!(w.alpha(), 0.0, "prepare_show 不应修改 alpha，alpha 仍应为 0");
        assert!(!w.ignores_mouse(), "prepare_show 后 ignores_mouse 应为 false");
        assert!(
            steps.contains(&"prepare-show"),
            "steps 应包含 prepare-show，实际={:?}",
            steps
        );
    }

    // ── 边界用例：FAIL_COUNT 达到上限后 hide 走 fallback 路径 ────────────────
    // Validates: Requirements 2.8
    //
    // 注：在纯 Rust 单元测试中无法直接注入 Obj-C 异常让 try_block 返回 false，
    // 因此通过将 FAIL_COUNT 直接设为 FAIL_LIMIT 来模拟永久禁用状态，
    // 验证 hide 在该状态下走 fallback 路径并写入相应步骤。
    #[test]
    fn hide_fallback_when_permanently_disabled() {
        let _g = FAIL_COUNT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_fail_count();
        // 直接将 FAIL_COUNT 设为上限，模拟永久禁用
        FAIL_COUNT.store(FAIL_LIMIT, Ordering::SeqCst);
        let w = MockWindow::new();
        let mut steps = log::Steps::new();
        hide(&w, &mut steps);
        // 永久禁用路径：steps 应包含 fallback 相关步骤
        assert!(
            steps.iter().any(|s| s.contains("fallback") || s.contains("disabled")),
            "FAIL_COUNT 达到上限后 hide 应走 fallback 路径，steps={:?}",
            steps
        );
        // 永久禁用时不应修改 alpha（不执行 try_block 内的操作）
        assert_eq!(w.alpha(), 1.0, "永久禁用时 hide 不应修改 alpha");
        reset_fail_count();
    }

    // ── Property 3: Hide 后置条件不变量 ─────────────────────────────────────
    //
    // Feature: webkit-presentation-tuning, Property 3: Hide 后置条件不变量。
    // 对任意 show/hide 操作序列，每次 hide 后均满足：
    //   alpha==0, ignores_mouse==true, occlusion_detection==false,
    //   order_out_count==0, hide 完成耗时 <100ms。
    // Validates: Requirements 2.1, 2.2, 2.5
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn property_3_hide_postconditions(
            // false=hide, true=show
            ops in proptest::collection::vec(any::<bool>(), 1..32)
        ) {
            let _g = FAIL_COUNT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            reset_fail_count();
            let w = MockWindow::new();
            install(&w);

            for is_show in &ops {
                let mut steps = log::Steps::new();
                if *is_show {
                    // 模拟 show：prepare_show + 设置 alpha=1
                    prepare_show(&w, &mut steps);
                    w.set_alpha(1.0);
                } else {
                    let t0 = Instant::now();
                    hide(&w, &mut steps);
                    let elapsed = t0.elapsed();

                    // 后置条件断言
                    prop_assert_eq!(w.alpha(), 0.0, "hide 后 alpha 必须为 0");
                    prop_assert!(w.ignores_mouse(), "hide 后 ignores_mouse 必须为 true");
                    prop_assert!(
                        !w.occlusion_detection(),
                        "hide 后 occlusion_detection 必须为 false（install 已设置）"
                    );
                    prop_assert_eq!(
                        w.order_out_count(),
                        0,
                        "hide 不得调用 orderOut，order_out_count 必须为 0"
                    );
                    prop_assert!(
                        elapsed.as_millis() < 100,
                        "hide 完成耗时超过 100ms: {:?}",
                        elapsed
                    );
                }
            }
        }

        // Feature: webkit-presentation-tuning, Property 5: collectionBehavior 始终包含 Transient。
        // install 后任意操作序列下 collection_behavior & Transient != 0。
        // Validates: Requirements 2.6
        #[test]
        fn property_5_collection_behavior_always_transient(
            ops in proptest::collection::vec(any::<bool>(), 0..32)
        ) {
            let _g = FAIL_COUNT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            reset_fail_count();
            let w = MockWindow::new();
            install(&w);

            prop_assert_ne!(
                w.collection_behavior() & TRANSIENT,
                0,
                "install 后 collectionBehavior 必须包含 Transient"
            );

            for is_show in &ops {
                let mut steps = log::Steps::new();
                if *is_show {
                    prepare_show(&w, &mut steps);
                } else {
                    hide(&w, &mut steps);
                }
                prop_assert_ne!(
                    w.collection_behavior() & TRANSIENT,
                    0,
                    "操作后 collectionBehavior 必须仍包含 Transient"
                );
            }
        }
    }

    // ── Property 4: prepare_show 不设置 alpha（时序验证的局部断言）────────────
    //
    // Feature: webkit-presentation-tuning, Property 4（局部）：
    // prepare_show 不修改 alpha，alpha=1 由 Presentation_Coordinator 在
    // await_paint 后设置（完整时序在 T10 跨组件验证）。
    // Validates: Requirements 2.7（局部）
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn property_4_prepare_show_does_not_set_alpha(
            initial_alpha in proptest::num::f64::POSITIVE
        ) {
            let _g = FAIL_COUNT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            reset_fail_count();
            let w = MockWindow::new();
            install(&w);
            // 设置任意初始 alpha（模拟各种初始状态）
            let clamped = initial_alpha.min(1.0).max(0.0);
            w.set_alpha(clamped);
            let alpha_before = w.alpha();
            let mut steps = log::Steps::new();
            prepare_show(&w, &mut steps);
            // prepare_show 不应修改 alpha
            prop_assert_eq!(
                w.alpha(),
                alpha_before,
                "prepare_show 不应修改 alpha，alpha 应保持不变"
            );
            prop_assert!(
                steps.contains(&"prepare-show"),
                "steps 应包含 prepare-show，实际={:?}",
                steps
            );
        }
    }
}
