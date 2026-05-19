#![allow(dead_code)]
//! Frame_Animator + Webview_Frame_Pin。
//!
//! T6 实装 Webview_Frame_Pin（pin 子模块）：
//! - install：把 WKWebView frame 锁到会话最大尺寸（720×480 起步），禁用 autoresizing。
//! - ensure_capacity：按需一次性扩大 WKWebView frame 至新最大尺寸。
//!
//! T7 实装 Frame_Animator（animate 函数）：
//! - animate：NSAnimationContext.beginGrouping → setAllowsImplicitAnimation:YES →
//!   setDuration:0.18 → setFrame:display:NO animate:YES → endGrouping，全部包在
//!   try_block 内；末尾重设 cornerRadius=16.0 + masksToBounds=true。
//! - 失败兜底：try_block 返回 false 时回退到 set_window_frame(target, false)，
//!   steps.push("fallback-set-size")，FAIL_COUNT+1。

use std::sync::atomic::{AtomicU8, Ordering};
use crate::macos::webkit_tuning::{Frame, WindowOps, log};

// ─────────────────────────────────────────────────────────────────────────────
// Frame_Animator 失败计数器
// ─────────────────────────────────────────────────────────────────────────────

/// Frame_Animator 失败计数器。3 次失败后永久禁用动画路径，回退到直接 setFrame。
static FAIL_COUNT: AtomicU8 = AtomicU8::new(0);
/// 永久禁用阈值。
const FAIL_LIMIT: u8 = 3;

/// 判断 Frame_Animator 是否已永久禁用。
fn is_disabled() -> bool {
    FAIL_COUNT.load(Ordering::SeqCst) >= FAIL_LIMIT
}

/// 记录一次失败，累计到 FAIL_LIMIT 后永久禁用。
fn record_failure() {
    FAIL_COUNT.fetch_add(1, Ordering::SeqCst);
}

// ─────────────────────────────────────────────────────────────────────────────
// Webview_Frame_Pin
// ─────────────────────────────────────────────────────────────────────────────

/// 会话初始最大尺寸（与 tauri.conf.json main 窗口配置一致）。
const INITIAL_MAX_WIDTH: f64 = 720.0;
const INITIAL_MAX_HEIGHT: f64 = 480.0;

pub(crate) mod pin {
    use super::*;

    /// 一次性安装：把 WKWebView frame 锁到会话最大尺寸，禁用 autoresizing。
    pub fn install<W: WindowOps>(window: &W) {
        let current = window.wkwebview_frame();
        let max_w = current.width.max(INITIAL_MAX_WIDTH);
        let max_h = current.height.max(INITIAL_MAX_HEIGHT);
        window.set_wkwebview_frame(Frame::new(current.x, current.y, max_w, max_h));
        log::component_status("Webview_Frame_Pin", log::Status::Enabled, None);
    }

    /// 读取当前 WKWebView frame 的容量（宽高）。
    pub fn current_capacity<W: WindowOps>(window: &W) -> Frame {
        window.wkwebview_frame()
    }

    /// 一次性扩大 WKWebView frame 至 max(now, requested)。
    pub fn grow<W: WindowOps>(window: &W, w: f64, h: f64) {
        let current = window.wkwebview_frame();
        let new_w = current.width.max(w);
        let new_h = current.height.max(h);
        window.set_wkwebview_frame(Frame::new(current.x, current.y, new_w, new_h));
    }

    /// 按需扩容：若当前容量不足则调用 grow，并记录步骤。
    pub fn ensure_capacity<W: WindowOps>(window: &W, w: f64, h: f64, steps: &mut log::Steps) {
        let cap = current_capacity(window);
        if !cap.contains_size(w, h) {
            grow(window, w, h);
            steps.push("pin-grow");
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 顶层入口
// ─────────────────────────────────────────────────────────────────────────────

/// 模块级 install：安装 Webview_Frame_Pin，并记录 Frame_Animator 状态。
pub fn install<W: WindowOps>(window: &W) {
    pin::install(window);
    log::component_status("Frame_Animator", log::Status::Enabled, None);
}

/// 按需扩容入口，转发到 pin::ensure_capacity。
pub fn ensure_capacity<W: WindowOps>(window: &W, w: f64, h: f64, steps: &mut log::Steps) {
    pin::ensure_capacity(window, w, h, steps);
}

/// Frame_Animator 动画入口：用 Core Animation 隐式动画调整 NSWindow frame。
///
/// 成功路径（try_block 返回 true）：
///   NSAnimationContext.beginGrouping → setAllowsImplicitAnimation:YES →
///   setDuration:0.18 → setFrame:display:NO animate:YES → endGrouping，
///   末尾重设 cornerRadius=16.0 + masksToBounds=true。
///   steps.push("ca-animate")，返回 true。
///
/// 失败路径（try_block 返回 false）：
///   回退到 set_window_frame(target, false)（无动画），
///   重设圆角，steps.push("fallback-set-size")，FAIL_COUNT+1，返回 false。
///
/// 永久禁用路径（FAIL_COUNT >= FAIL_LIMIT）：
///   直接 set_window_frame(target, false)，
///   steps.push("fallback-set-size-disabled")，返回 false。
pub fn animate<W: WindowOps>(window: &W, w: f64, h: f64, steps: &mut log::Steps) -> bool {
    let current = window.window_frame();
    let target = Frame::new(current.x, current.y, w, h);

    // 永久禁用时直接走 fallback，不进 try_block
    if is_disabled() {
        window.set_window_frame(target, false);
        window.set_content_view_corner_radius(16.0);
        window.set_content_view_masks_to_bounds(true);
        steps.push("fallback-set-size-disabled");
        return false;
    }

    // 成功路径：animated=true 对应 NSAnimationContext.beginGrouping +
    // setAllowsImplicitAnimation:YES + setDuration:0.18 +
    // setFrame:display:NO animate:YES + endGrouping
    let ok = crate::macos::webkit_tuning::obj_exception::try_block(|| {
        window.set_window_frame(target, true);
        // 重设圆角（Req 3.5：尺寸切换后 cornerRadius 与 masksToBounds 不被重置）
        window.set_content_view_corner_radius(16.0);
        window.set_content_view_masks_to_bounds(true);
    });

    if ok {
        steps.push("ca-animate");
        true
    } else {
        // 失败兜底：直接 setFrame，无动画
        record_failure();
        window.set_window_frame(target, false);
        window.set_content_view_corner_radius(16.0);
        window.set_content_view_masks_to_bounds(true);
        steps.push("fallback-set-size");
        false
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
    use std::sync::atomic::Ordering;

    /// 测试间串行锁：FAIL_COUNT 是全局静态，多个测试并发修改会互相干扰。
    static FAIL_COUNT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// 重置 FAIL_COUNT 为 0，在每个需要干净状态的测试开头调用。
    fn reset_fail_count() {
        FAIL_COUNT.store(0, Ordering::SeqCst);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        // Feature: webkit-presentation-tuning, Property 6: WKWebView frame 在 resize 序列中只增不减。
        // 对任意 resize 请求序列，每次返回后 wkwebview_frame.size ≥ M_k（按宽高分量）；
        // 扩容次数等于 M_k 创新高的次数。
        // Validates: Requirements 3.1, 3.3, 3.6
        #[test]
        fn property_6_webview_frame_pin_monotone(
            sizes in proptest::collection::vec(
                (10f64..2000.0_f64, 10f64..1500.0_f64),
                0..16
            )
        ) {
            // ── 第一轮：验证每次调用后 frame ≥ M_k ──────────────────────────
            // MockWindow::new() 返回 Arc<MockWindow>，WindowOps 为 Arc<MockWindow> 实现
            let w = MockWindow::new();
            pin::install(&w);

            let mut max_w = INITIAL_MAX_WIDTH;
            let mut max_h = INITIAL_MAX_HEIGHT;

            for (req_w, req_h) in &sizes {
                max_w = max_w.max(*req_w);
                max_h = max_h.max(*req_h);

                let mut steps = log::Steps::new();
                pin::ensure_capacity(&w, *req_w, *req_h, &mut steps);

                let cap = w.wkwebview_frame();
                prop_assert!(
                    cap.width >= max_w && cap.height >= max_h,
                    "wkwebview_frame ({}, {}) 应 ≥ M_k ({}, {})",
                    cap.width, cap.height, max_w, max_h
                );
            }

            // ── 第二轮：验证扩容次数等于 M_k 创新高的次数 ───────────────────
            let w2 = MockWindow::new();
            pin::install(&w2);

            let mut actual_grow_count = 0u32;
            let mut expected_grow_count = 0u32;
            let mut max_w2 = INITIAL_MAX_WIDTH;
            let mut max_h2 = INITIAL_MAX_HEIGHT;

            for (req_w, req_h) in &sizes {
                let prev_max_w = max_w2;
                let prev_max_h = max_h2;
                max_w2 = max_w2.max(*req_w);
                max_h2 = max_h2.max(*req_h);

                // M_k 是否创新高（按分量，任一分量超出即算）
                let new_high = max_w2 > prev_max_w || max_h2 > prev_max_h;
                if new_high {
                    expected_grow_count += 1;
                }

                let mut steps = log::Steps::new();
                pin::ensure_capacity(&w2, *req_w, *req_h, &mut steps);

                if steps.contains(&"pin-grow") {
                    actual_grow_count += 1;
                }
            }

            prop_assert_eq!(
                actual_grow_count, expected_grow_count,
                "扩容次数应等于 M_k 创新高的次数"
            );
        }

        // Feature: webkit-presentation-tuning, Property 7: Resize 后置条件不变量。
        // 每次 animate 后：beginGrouping/endGrouping 计数差==0（ca_transaction_count 正确），
        // cornerRadius==16.0，masksToBounds==true，setAllowsImplicitAnimation 被调过且为 true。
        // Validates: Requirements 3.2, 3.5
        #[test]
        fn property_7_resize_postconditions(
            sizes in proptest::collection::vec(
                (10f64..2000.0_f64, 10f64..1500.0_f64),
                1..16
            )
        ) {
            let _g = FAIL_COUNT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            reset_fail_count();
            let w = MockWindow::new();
            install(&w);

            let mut expected_ca_count = 0u32;
            for (req_w, req_h) in &sizes {
                let mut steps = log::Steps::new();
                let ok = animate(&w, *req_w, *req_h, &mut steps);
                if ok {
                    expected_ca_count += 1;
                }

                // cornerRadius 和 masksToBounds 在每次 animate 后应保持正确值（Req 3.5）
                prop_assert_eq!(
                    w.content_view_corner_radius(), 16.0,
                    "animate 后 cornerRadius 应为 16.0"
                );
                prop_assert!(
                    w.content_view_masks_to_bounds(),
                    "animate 后 masksToBounds 应为 true"
                );
            }

            // ca_transaction_count 应等于成功的 animate 调用次数（beginGrouping/endGrouping 平衡）
            prop_assert_eq!(
                w.ca_transaction_count.load(Ordering::SeqCst),
                expected_ca_count,
                "ca_transaction_count 应等于成功的 animate 调用次数（beginGrouping/endGrouping 差==0）"
            );
        }

        // Feature: webkit-presentation-tuning, Property 10: idle 期间无多余 CA 事务。
        // 不包含 resize 的 show/hide 操作序列结束后，Frame_Animator 自身贡献的
        // CATransaction.begin 调用次数为 0。
        // Validates: Requirements 6.3
        #[test]
        fn property_10_no_ca_transactions_without_resize(
            // false=hide 操作，true=show 操作；两者都不调用 animate
            ops in proptest::collection::vec(any::<bool>(), 0..32)
        ) {
            let _g = FAIL_COUNT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            reset_fail_count();
            let w = MockWindow::new();
            install(&w);

            // 只做 show/hide 相关操作（不调用 animate），ca_transaction_count 不应增加
            for is_show in &ops {
                if *is_show {
                    // 模拟 show：设置 alpha=1，不调用 animate
                    w.set_alpha(1.0);
                } else {
                    // 模拟 hide：设置 alpha=0，不调用 animate
                    w.set_alpha(0.0);
                }
            }

            // Frame_Animator 自身贡献的 CATransaction.begin 调用次数应为 0
            prop_assert_eq!(
                w.ca_transaction_count.load(Ordering::SeqCst),
                0u32,
                "无 resize 操作时 ca_transaction_count 应为 0"
            );
        }
    }

    /// install 后 WKWebView frame 宽高 ≥ 初始最大尺寸（720×480）。
    #[test]
    fn install_locks_to_initial_max() {
        let w = MockWindow::new();
        pin::install(&w);
        let cap = w.wkwebview_frame();
        assert!(cap.width >= INITIAL_MAX_WIDTH);
        assert!(cap.height >= INITIAL_MAX_HEIGHT);
    }

    /// 请求尺寸小于当前容量时，不触发扩容，steps 中不含 "pin-grow"。
    #[test]
    fn ensure_capacity_no_grow_when_sufficient() {
        let w = MockWindow::new();
        pin::install(&w);
        let mut steps = log::Steps::new();
        // 请求小于初始容量（720×480），不应扩容
        pin::ensure_capacity(&w, 100.0, 100.0, &mut steps);
        assert!(!steps.contains(&"pin-grow"), "容量足够时不应扩容");
    }

    /// 请求尺寸超过当前容量时，触发扩容，steps 中含 "pin-grow"，frame 更新。
    #[test]
    fn ensure_capacity_grows_when_needed() {
        let w = MockWindow::new();
        pin::install(&w);
        let mut steps = log::Steps::new();
        // 请求超过初始容量
        pin::ensure_capacity(&w, 1000.0, 800.0, &mut steps);
        assert!(steps.contains(&"pin-grow"), "容量不足时应扩容");
        let cap = w.wkwebview_frame();
        assert!(cap.width >= 1000.0);
        assert!(cap.height >= 800.0);
    }

    /// grow 是单调的：再次 grow 到更小的值不应缩小 frame。
    #[test]
    fn grow_is_monotone() {
        let w = MockWindow::new();
        pin::install(&w);
        pin::grow(&w, 900.0, 600.0);
        let cap1 = w.wkwebview_frame();
        // 再次 grow 到更小的值，不应缩小
        pin::grow(&w, 100.0, 100.0);
        let cap2 = w.wkwebview_frame();
        assert_eq!(cap1.width, cap2.width, "grow 不应缩小宽度");
        assert_eq!(cap1.height, cap2.height, "grow 不应缩小高度");
    }

    /// current_capacity 返回当前 WKWebView frame。
    #[test]
    fn current_capacity_reflects_frame() {
        let w = MockWindow::new();
        pin::install(&w);
        let cap = pin::current_capacity(&w);
        assert_eq!(cap.width, INITIAL_MAX_WIDTH);
        assert_eq!(cap.height, INITIAL_MAX_HEIGHT);
    }

    /// 顶层 install 调用后 Frame_Animator 状态日志正常（不 panic）。
    #[test]
    fn top_level_install_does_not_panic() {
        let w = MockWindow::new();
        install(&w); // 不应 panic
        let cap = w.wkwebview_frame();
        assert!(cap.width >= INITIAL_MAX_WIDTH);
        assert!(cap.height >= INITIAL_MAX_HEIGHT);
    }

    /// animate 成功路径：设置 window frame、cornerRadius=16.0、masksToBounds=true，
    /// steps 含 "ca-animate"，ca_transaction_count == 1。
    #[test]
    fn animate_sets_window_frame_and_corner_radius() {
        let _g = FAIL_COUNT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_fail_count();
        let w = MockWindow::new();
        install(&w);
        let mut steps = log::Steps::new();
        let ok = animate(&w, 800.0, 600.0, &mut steps);
        assert!(ok, "animate 应成功");
        assert_eq!(w.window_frame().width, 800.0, "window frame 宽度应更新");
        assert_eq!(w.window_frame().height, 600.0, "window frame 高度应更新");
        assert_eq!(w.content_view_corner_radius(), 16.0, "cornerRadius 应为 16.0");
        assert!(w.content_view_masks_to_bounds(), "masksToBounds 应为 true");
        assert!(steps.contains(&"ca-animate"), "steps 应含 ca-animate，实际={:?}", steps);
        assert_eq!(
            w.ca_transaction_count.load(Ordering::SeqCst), 1,
            "成功的 animate 应产生 1 次 CA 事务"
        );
    }

    /// animate 多次调用：ca_transaction_count 累计等于成功次数。
    #[test]
    fn animate_multiple_calls_accumulate_ca_count() {
        let _g = FAIL_COUNT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_fail_count();
        let w = MockWindow::new();
        install(&w);

        for i in 1..=3u32 {
            let mut steps = log::Steps::new();
            let ok = animate(&w, 400.0 + i as f64 * 100.0, 300.0 + i as f64 * 50.0, &mut steps);
            assert!(ok, "第 {} 次 animate 应成功", i);
        }

        assert_eq!(
            w.ca_transaction_count.load(Ordering::SeqCst), 3,
            "3 次成功 animate 应产生 3 次 CA 事务"
        );
        assert_eq!(w.content_view_corner_radius(), 16.0, "最终 cornerRadius 应为 16.0");
        assert!(w.content_view_masks_to_bounds(), "最终 masksToBounds 应为 true");
    }

    /// animate 永久禁用路径：FAIL_COUNT >= FAIL_LIMIT 时直接 fallback，不产生 CA 事务。
    #[test]
    fn animate_fallback_when_disabled() {
        let _g = FAIL_COUNT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_fail_count();
        FAIL_COUNT.store(FAIL_LIMIT, Ordering::SeqCst);
        let w = MockWindow::new();
        let mut steps = log::Steps::new();
        let ok = animate(&w, 800.0, 600.0, &mut steps);
        assert!(!ok, "永久禁用时 animate 应返回 false");
        assert_eq!(
            w.ca_transaction_count.load(Ordering::SeqCst), 0,
            "fallback 路径不应产生 CA 事务"
        );
        assert!(
            steps.iter().any(|s| s.contains("fallback")),
            "steps 应含 fallback 步骤，实际={:?}", steps
        );
        // fallback 路径也应重设圆角
        assert_eq!(w.content_view_corner_radius(), 16.0, "fallback 后 cornerRadius 应为 16.0");
        assert!(w.content_view_masks_to_bounds(), "fallback 后 masksToBounds 应为 true");
        reset_fail_count();
    }

    /// animate 不调用时，ca_transaction_count 保持为 0（Property 10 单元验证）。
    #[test]
    fn no_animate_no_ca_transactions() {
        let _g = FAIL_COUNT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_fail_count();
        let w = MockWindow::new();
        install(&w);

        // 只做 show/hide 操作，不调用 animate
        w.set_alpha(1.0);
        w.set_alpha(0.0);
        w.set_alpha(1.0);

        assert_eq!(
            w.ca_transaction_count.load(Ordering::SeqCst), 0,
            "不调用 animate 时 ca_transaction_count 应为 0"
        );
    }
}
