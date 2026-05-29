use super::*;
use crate::macos::webkit_tuning::test_support::{MockPresentationBridge, MockWindow};
use proptest::prelude::*;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

struct CapturingLogger {
    records: Mutex<Vec<String>>,
}

impl ::log::Log for CapturingLogger {
    fn enabled(&self, _: &::log::Metadata) -> bool { true }
    fn log(&self, record: &::log::Record) {
        if record.target() == "webkit_tuning" {
            self.records.lock().unwrap().push(record.args().to_string());
        }
    }
    fn flush(&self) {}
}

static SINK: OnceLock<&'static CapturingLogger> = OnceLock::new();

static TEST_LOCK: Mutex<()> = Mutex::new(());

fn init_sink() -> &'static CapturingLogger {
    SINK.get_or_init(|| {
        let logger: &'static CapturingLogger = Box::leak(Box::new(CapturingLogger {
            records: Mutex::new(Vec::new()),
        }));
        let _ = ::log::set_logger(logger);
        ::log::set_max_level(::log::LevelFilter::Trace);
        logger
    })
}

fn clear_sink(sink: &CapturingLogger) {
    sink.records.lock().unwrap().clear();
}

fn snapshot_sink(sink: &CapturingLogger) -> Vec<String> {
    sink.records.lock().unwrap().clone()
}

#[test]
fn install_with_toggle_disabled_does_not_call_components() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    toggle::override_enabled(false);

    let w = MockWindow::new();
    install_with(&w);

    assert!(w.occlusion_detection(), "toggle 禁用时 occlusion_detection 应保持 true（throttling 未安装）");
    assert_eq!(w.collection_behavior(), 0, "toggle 禁用时 collectionBehavior 应保持 0（throttling 未安装）");

    toggle::clear_override();
}

#[test]
fn install_with_toggle_enabled_calls_all_components() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    toggle::override_enabled(true);

    let w = MockWindow::new();
    install_with(&w);

    assert!(!w.occlusion_detection(), "toggle 启用时 occlusion_detection 应为 false（throttling 已安装）");
    assert_ne!(w.collection_behavior() & (1 << 2), 0, "toggle 启用时 collectionBehavior 应含 Transient");
    let cap = w.wkwebview_frame();
    assert!(cap.width >= 720.0 && cap.height >= 480.0, "toggle 启用时 wkwebview_frame 应 ≥ 720×480");

    toggle::clear_override();
}

#[test]
fn show_main_with_toggle_disabled_pushes_legacy_show() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    toggle::override_enabled(false);

    let w = MockWindow::new();
    let bridge = MockPresentationBridge::new(true, 0);
    let mut steps = log::Steps::new();
    show_main_with(&w, &bridge, &mut steps);

    assert!(steps.contains(&"legacy-show"), "toggle 禁用时 steps 应含 legacy-show，实际={:?}", steps);
    assert_eq!(bridge.schedule_count(), 0, "toggle 禁用时不应调用 bridge");

    toggle::clear_override();
}

#[test]
fn hide_main_with_toggle_disabled_pushes_legacy_hide() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    toggle::override_enabled(false);

    let w = MockWindow::new();
    let mut steps = log::Steps::new();
    hide_main_with(&w, &mut steps);

    assert!(steps.contains(&"legacy-hide"), "toggle 禁用时 steps 应含 legacy-hide，实际={:?}", steps);

    toggle::clear_override();
}

#[test]
fn resize_main_with_toggle_disabled_pushes_legacy_set_size() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    toggle::override_enabled(false);

    let w = MockWindow::new();
    let mut steps = log::Steps::new();
    resize_main_with(&w, 800.0, 600.0, &mut steps);

    assert!(steps.contains(&"legacy-set-size"), "toggle 禁用时 steps 应含 legacy-set-size，实际={:?}", steps);

    toggle::clear_override();
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn property_2_label_guard_bridge_not_called_for_non_main(
        label in proptest::sample::select(vec!["main", "screenshot", "x", ""])
    ) {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        toggle::override_enabled(true);

        let bridge = MockPresentationBridge::new(true, 0);
        let w = MockWindow::new();
        w.set_alpha(0.0);

        if label == "main" {
            let mut steps = log::Steps::new();
            show_main_with(&w, &bridge, &mut steps);
            prop_assert!(
                bridge.schedule_count() > 0,
                "main 窗口 show_main_with 应调用 bridge.schedule"
            );
        } else {
            prop_assert_eq!(
                bridge.schedule_count(),
                0,
                "非 main 窗口不应调用 bridge.schedule，label={}", label
            );
        }

        toggle::clear_override();
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn property_4_pre_show_alpha_timing(d_ms in 0u64..200u64) {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        toggle::override_enabled(true);

        let w = MockWindow::new();
        w.set_alpha(0.0);

        let paint_will_arrive = d_ms <= 80;
        let bridge = MockPresentationBridge::new(paint_will_arrive, d_ms.min(80));

        let mut steps = log::Steps::new();

        let t_pre = Instant::now();
        show_main_with(&w, &bridge, &mut steps);
        let t_alpha1 = w.last_alpha_set_at.lock().unwrap().unwrap();

        let max_gap_ms = 16u128 + 80 + 32;
        let gap = t_alpha1.duration_since(t_pre);
        prop_assert!(
            gap.as_millis() <= max_gap_ms,
            "alpha=1 应在 pre-show 后 {}ms 内，实际={:?}",
            max_gap_ms, gap
        );

        prop_assert!(
            steps.contains(&"pre-show"),
            "steps 应含 pre-show，实际={:?}", steps
        );

        toggle::clear_override();
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn property_11_install_teardown_no_observer_leak(n in 0u32..32) {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        toggle::override_enabled(true);

        for _ in 0..n {
            let w = MockWindow::new();
            install_with(&w);
            uninstall_for_test(&w);
            prop_assert_eq!(
                w.observer_count(), 0,
                "teardown 后 observer_count 应为 0"
            );
        }

        toggle::clear_override();
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn property_13_install_calls_correct_components(
        toggle_enabled in any::<bool>()
    ) {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        toggle::override_enabled(toggle_enabled);

        let w = MockWindow::new();
        install_with(&w);

        if toggle_enabled {
            prop_assert!(!w.occlusion_detection(), "toggle 启用时 occlusion_detection 应为 false");
            prop_assert_ne!(w.collection_behavior() & (1 << 2), 0, "toggle 启用时应含 Transient");
            let cap = w.wkwebview_frame();
            prop_assert!(cap.width >= 720.0 && cap.height >= 480.0, "toggle 启用时 wkwebview_frame 应 ≥ 720×480");
        } else {
            prop_assert!(w.occlusion_detection(), "toggle 禁用时 occlusion_detection 应保持 true");
            prop_assert_eq!(w.collection_behavior(), 0, "toggle 禁用时 collectionBehavior 应保持 0");
        }

        toggle::clear_override();
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn property_14_event_steps_correspond_to_events(
        events in proptest::collection::vec(
            proptest::prop_oneof![
                Just(0u8),
                Just(1u8),
                Just(2u8),
            ],
            0..32
        )
    ) {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        toggle::override_enabled(true);

        let w = MockWindow::new();
        install_with(&w);

        let n = events.len();
        let bridge = MockPresentationBridge::new(true, 0);
        let mut event_count = 0usize;

        for ev in &events {
            let t0 = Instant::now();
            match ev {
                0 => {
                    w.set_alpha(0.0);
                    let mut steps = log::Steps::new();
                    show_main_with(&w, &bridge, &mut steps);
                    let elapsed = t0.elapsed();
                    prop_assert!(!steps.is_empty(), "show 事件应产生非空 steps");
                    prop_assert!(
                        elapsed.as_millis() <= 200,
                        "show_main_with 耗时应 ≤200ms，实际={:?}", elapsed
                    );
                    event_count += 1;
                }
                1 => {
                    let mut steps = log::Steps::new();
                    hide_main_with(&w, &mut steps);
                    prop_assert!(!steps.is_empty(), "hide 事件应产生非空 steps");
                    event_count += 1;
                }
                _ => {
                    let mut steps = log::Steps::new();
                    resize_main_with(&w, 800.0, 600.0, &mut steps);
                    prop_assert!(!steps.is_empty(), "resize 事件应产生非空 steps");
                    event_count += 1;
                }
            }
        }

        prop_assert_eq!(event_count, n, "事件处理次数应等于事件序列长度");

        toggle::clear_override();
    }
}
