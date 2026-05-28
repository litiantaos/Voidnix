//! PBT 测试支持：MockWindow + MockPresentationBridge。
//!
//! 本模块仅在 `#[cfg(test)]` 下编译，提供：
//! - [`MockWindow`]：内存实现的 [`WindowOps`]，记录所有操作的调用次数与最后设置的值。
//! - [`MockPresentationBridge`]：由测试控制 `paint_will_arrive` 与 `delay_ms`，
//!   模拟 SPI 可用/不可用两种场景。
//!
//! 设计原则：
//! - 不依赖 mockall 宏（避免 trait 方法签名与 mockall 生成代码不同步的问题）；
//!   直接用原子变量 + Mutex 手写实现，更易于 PBT 断言。
//! - 所有字段均为 `pub`，测试可直接读取计数器与状态，无需额外 getter。
//! - `Arc<MockWindow>` 实现 `WindowOps`，方便多处持有同一 mock 实例。

use super::{Frame, PresentationBridge, WindowOps};
use std::sync::{
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

// ─────────────────────────────────────────────────────────────────────────────
// MockWindow
// ─────────────────────────────────────────────────────────────────────────────

/// 内存 MockWindow，实现 [`WindowOps`] 全部方法。
///
/// 所有字段均为 `pub`，测试可直接读取计数器与状态。
/// 通过 [`MockWindow::new`] 创建并包装在 `Arc` 中使用。
#[derive(Debug)]
pub(crate) struct MockWindow {
    // ── NSWindow alpha ──────────────────────────────────────────────────────
    /// alphaValue 的 f64 位表示（AtomicU64 存储）。
    pub alpha_bits: AtomicU64,
    /// set_alpha 被调用的次数。
    pub set_alpha_count: AtomicU32,
    /// 最后一次 set_alpha 的时刻（用于 Property 4 时序断言）。
    pub last_alpha_set_at: Mutex<Option<Instant>>,

    // ── NSWindow frame ──────────────────────────────────────────────────────
    /// NSWindow 的屏幕坐标 frame。
    pub window_frame: Mutex<Frame>,
    /// set_window_frame 被调用的次数。
    pub set_frame_count: AtomicU32,
    /// set_window_frame 中 animated=true 的调用次数（对应 CATransaction）。
    pub ca_transaction_count: AtomicU32,

    // ── 鼠标事件穿透 ────────────────────────────────────────────────────────
    /// ignoresMouseEvents 当前值。
    pub ignores_mouse: AtomicBool,

    // ── orderOut / orderFront 计数 ──────────────────────────────────────────
    /// orderOut 被调用的次数（PBT 断言 hide 路径不调用 orderOut）。
    pub order_out_count: AtomicU32,
    /// orderFrontRegardless 被调用的次数。
    pub order_front_count: AtomicU32,

    // ── 遮挡检测 ────────────────────────────────────────────────────────────
    /// windowOcclusionDetectionEnabled 当前值。
    pub occlusion_detection: AtomicBool,

    // ── collectionBehavior ──────────────────────────────────────────────────
    /// NSWindow.collectionBehavior 原始位掩码。
    pub collection_behavior: AtomicU64,

    // ── contentView 圆角 ────────────────────────────────────────────────────
    /// contentView.layer.cornerRadius 的 f64 位表示。
    pub corner_radius_bits: AtomicU64,

    // ── contentView masksToBounds ───────────────────────────────────────────
    /// contentView.layer.masksToBounds 当前值。
    pub masks_to_bounds: AtomicBool,

    // ── WKWebView frame ─────────────────────────────────────────────────────
    /// WKWebView 的 frame（相对于 contentView 坐标系）。
    pub wkwebview_frame: Mutex<Frame>,

    // ── observer 计数 ───────────────────────────────────────────────────────
    /// 当前已注册的 NSNotification + KVO observer 数量。
    pub observer_count: AtomicU32,

    // ── key window ──────────────────────────────────────────────────────────
    /// makeKeyWindow 被调用的次数。
    pub make_key_count: AtomicU32,
    /// resignKeyWindow 被调用的次数。
    pub resign_key_count: AtomicU32,
}

impl Default for MockWindow {
    fn default() -> Self {
        Self {
            // 默认 alpha = 1.0（完全不透明）
            alpha_bits: AtomicU64::new(f64::to_bits(1.0)),
            set_alpha_count: AtomicU32::new(0),
            last_alpha_set_at: Mutex::new(None),

            // 默认 window frame = (0, 0, 720, 480)
            window_frame: Mutex::new(Frame::new(0.0, 0.0, 720.0, 480.0)),
            set_frame_count: AtomicU32::new(0),
            ca_transaction_count: AtomicU32::new(0),

            ignores_mouse: AtomicBool::new(false),

            order_out_count: AtomicU32::new(0),
            order_front_count: AtomicU32::new(0),

            // 默认启用遮挡检测（安装前的初始状态）
            occlusion_detection: AtomicBool::new(true),

            collection_behavior: AtomicU64::new(0),

            // 默认圆角 = 16.0（与 lib.rs setup 一致）
            corner_radius_bits: AtomicU64::new(f64::to_bits(16.0)),

            // 默认 masksToBounds = true
            masks_to_bounds: AtomicBool::new(true),

            // 默认 wkwebview frame = (0, 0, 720, 480)
            wkwebview_frame: Mutex::new(Frame::new(0.0, 0.0, 720.0, 480.0)),

            observer_count: AtomicU32::new(0),

            make_key_count: AtomicU32::new(0),
            resign_key_count: AtomicU32::new(0),
        }
    }
}

impl MockWindow {
    /// 创建一个包装在 `Arc` 中的 MockWindow，使用合理的默认初始状态。
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// 创建一个自定义初始 frame 的 MockWindow。
    pub fn with_frame(x: f64, y: f64, width: f64, height: f64) -> Arc<Self> {
        let w = Arc::new(Self::default());
        *w.window_frame.lock().unwrap() = Frame::new(x, y, width, height);
        *w.wkwebview_frame.lock().unwrap() = Frame::new(0.0, 0.0, width, height);
        w
    }

    // ── 便捷读取方法（避免测试中重复写 f64::from_bits）──────────────────────

    /// 读取当前 alphaValue（f64）。
    pub fn alpha(&self) -> f64 {
        f64::from_bits(self.alpha_bits.load(Ordering::SeqCst))
    }

    /// 读取当前 cornerRadius（f64）。
    pub fn corner_radius(&self) -> f64 {
        f64::from_bits(self.corner_radius_bits.load(Ordering::SeqCst))
    }
}

/// `Arc<MockWindow>` 实现 `WindowOps`，方便多处持有同一 mock 实例。
impl WindowOps for Arc<MockWindow> {
    // ── alpha ────────────────────────────────────────────────────────────────
    fn alpha(&self) -> f64 {
        f64::from_bits(self.alpha_bits.load(Ordering::SeqCst))
    }

    fn set_alpha(&self, v: f64) {
        self.alpha_bits.store(f64::to_bits(v), Ordering::SeqCst);
        self.set_alpha_count.fetch_add(1, Ordering::SeqCst);
        *self.last_alpha_set_at.lock().unwrap() = Some(Instant::now());
    }

    // ── window frame ─────────────────────────────────────────────────────────
    fn window_frame(&self) -> Frame {
        *self.window_frame.lock().unwrap()
    }

    fn set_window_frame(&self, f: Frame, animated: bool) {
        *self.window_frame.lock().unwrap() = f;
        self.set_frame_count.fetch_add(1, Ordering::SeqCst);
        if animated {
            // animated=true 对应 NSAnimationContext / CATransaction 路径
            self.ca_transaction_count.fetch_add(1, Ordering::SeqCst);
        }
    }

    // ── ignores mouse ────────────────────────────────────────────────────────
    fn ignores_mouse(&self) -> bool {
        self.ignores_mouse.load(Ordering::SeqCst)
    }

    fn set_ignores_mouse(&self, v: bool) {
        self.ignores_mouse.store(v, Ordering::SeqCst);
    }

    // ── order out / front ────────────────────────────────────────────────────
    fn order_out_count(&self) -> u32 {
        self.order_out_count.load(Ordering::SeqCst)
    }

    fn order_front(&self) {
        self.order_front_count.fetch_add(1, Ordering::SeqCst);
    }

    // ── occlusion detection ──────────────────────────────────────────────────
    fn occlusion_detection(&self) -> bool {
        self.occlusion_detection.load(Ordering::SeqCst)
    }

    fn set_occlusion_detection(&self, v: bool) {
        self.occlusion_detection.store(v, Ordering::SeqCst);
    }

    // ── collection behavior ──────────────────────────────────────────────────
    fn collection_behavior(&self) -> u64 {
        self.collection_behavior.load(Ordering::SeqCst)
    }

    fn set_collection_behavior(&self, v: u64) {
        self.collection_behavior.store(v, Ordering::SeqCst);
    }

    // ── corner radius ────────────────────────────────────────────────────────
    fn content_view_corner_radius(&self) -> f64 {
        f64::from_bits(self.corner_radius_bits.load(Ordering::SeqCst))
    }

    fn set_content_view_corner_radius(&self, r: f64) {
        self.corner_radius_bits.store(f64::to_bits(r), Ordering::SeqCst);
    }

    // ── masks to bounds ──────────────────────────────────────────────────────
    fn content_view_masks_to_bounds(&self) -> bool {
        self.masks_to_bounds.load(Ordering::SeqCst)
    }

    fn set_content_view_masks_to_bounds(&self, v: bool) {
        self.masks_to_bounds.store(v, Ordering::SeqCst);
    }

    // ── wkwebview frame ──────────────────────────────────────────────────────
    fn wkwebview_frame(&self) -> Frame {
        *self.wkwebview_frame.lock().unwrap()
    }

    fn set_wkwebview_frame(&self, f: Frame) {
        *self.wkwebview_frame.lock().unwrap() = f;
    }

    // ── observer count ───────────────────────────────────────────────────────
    fn observer_count(&self) -> u32 {
        self.observer_count.load(Ordering::SeqCst)
    }

    // ── key window ───────────────────────────────────────────────────────────
    fn make_key(&self) {
        self.make_key_count.fetch_add(1, Ordering::SeqCst);
    }

    fn resign_key(&self) {
        self.resign_key_count.fetch_add(1, Ordering::SeqCst);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// MockPresentationBridge
// ─────────────────────────────────────────────────────────────────────────────

/// MockPresentationBridge：由测试控制 paint 是否到达与延迟。
///
/// - `paint_will_arrive = true`：回调以 `ok=true` 触发（模拟 presentation 正常完成）。
/// - `paint_will_arrive = false`：回调以 `ok=false` 触发（模拟超时）。
/// - `delay_ms`：回调触发前的延迟毫秒数（0 表示立即在新线程中触发）。
/// - `spi_available = false`：`schedule` 直接返回 false，模拟 SPI 不可用。
pub(crate) struct MockPresentationBridge {
    /// paint 回调是否以 ok=true 触发。
    pub paint_will_arrive: bool,
    /// 回调触发前的延迟毫秒数。
    pub delay_ms: u64,
    /// SPI 是否可用；false 时 schedule 直接返回 false。
    pub spi_available: bool,
    /// schedule 被调用的次数（用于 Property 2 断言 label != "main" 时调用次数为 0）。
    pub schedule_count: AtomicU32,
}

impl MockPresentationBridge {
    /// 创建一个 SPI 可用的 MockPresentationBridge。
    ///
    /// - `paint_will_arrive`：回调是否以 ok=true 触发。
    /// - `delay_ms`：回调触发前的延迟毫秒数。
    pub fn new(paint_will_arrive: bool, delay_ms: u64) -> Self {
        Self {
            paint_will_arrive,
            delay_ms,
            spi_available: true,
            schedule_count: AtomicU32::new(0),
        }
    }

    /// 创建一个 SPI 不可用的 MockPresentationBridge（schedule 始终返回 false）。
    pub fn unavailable() -> Self {
        Self {
            paint_will_arrive: false,
            delay_ms: 0,
            spi_available: false,
            schedule_count: AtomicU32::new(0),
        }
    }

    /// 读取 schedule 被调用的次数。
    pub fn schedule_count(&self) -> u32 {
        self.schedule_count.load(Ordering::SeqCst)
    }
}

impl PresentationBridge for MockPresentationBridge {
    fn schedule(&self, _timeout_ms: u64, cb: Box<dyn FnOnce(bool) + Send>) -> bool {
        self.schedule_count.fetch_add(1, Ordering::SeqCst);

        if !self.spi_available {
            // SPI 不可用：直接返回 false，不触发回调
            return false;
        }

        let will_arrive = self.paint_will_arrive;
        let delay = self.delay_ms;

        // 在新线程中异步触发回调，模拟 native 侧的异步行为
        std::thread::spawn(move || {
            if delay > 0 {
                std::thread::sleep(Duration::from_millis(delay));
            }
            cb(will_arrive);
        });

        true
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 单元测试
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── MockWindow 基础功能测试 ──────────────────────────────────────────────

    /// MockWindow 实现 WindowOps 全部方法：alpha 读写。
    #[test]
    fn mock_window_alpha_read_write() {
        let w = MockWindow::new();
        assert_eq!(w.alpha(), 1.0, "默认 alpha 应为 1.0");
        w.set_alpha(0.0);
        assert_eq!(w.alpha(), 0.0);
        assert_eq!(w.set_alpha_count.load(Ordering::SeqCst), 1);
        w.set_alpha(0.5);
        assert_eq!(w.alpha(), 0.5);
        assert_eq!(w.set_alpha_count.load(Ordering::SeqCst), 2);
    }

    /// set_alpha 记录时间戳。
    #[test]
    fn mock_window_alpha_records_timestamp() {
        let w = MockWindow::new();
        assert!(w.last_alpha_set_at.lock().unwrap().is_none(), "初始无时间戳");
        let before = Instant::now();
        w.set_alpha(0.0);
        let after = Instant::now();
        let ts = w.last_alpha_set_at.lock().unwrap().unwrap();
        assert!(ts >= before && ts <= after, "时间戳应在调用前后之间");
    }

    /// MockWindow 实现 WindowOps 全部方法：window_frame 读写。
    #[test]
    fn mock_window_frame_read_write() {
        let w = MockWindow::new();
        let f = Frame::new(10.0, 20.0, 800.0, 600.0);
        w.set_window_frame(f, false);
        assert_eq!(w.window_frame(), f);
        assert_eq!(w.set_frame_count.load(Ordering::SeqCst), 1);
        assert_eq!(w.ca_transaction_count.load(Ordering::SeqCst), 0, "非 animated 不计 CA");
    }

    /// animated=true 时 ca_transaction_count 递增。
    #[test]
    fn mock_window_animated_frame_increments_ca_count() {
        let w = MockWindow::new();
        w.set_window_frame(Frame::new(0.0, 0.0, 800.0, 600.0), true);
        assert_eq!(w.ca_transaction_count.load(Ordering::SeqCst), 1);
        w.set_window_frame(Frame::new(0.0, 0.0, 900.0, 700.0), false);
        assert_eq!(w.ca_transaction_count.load(Ordering::SeqCst), 1, "非 animated 不再递增");
        w.set_window_frame(Frame::new(0.0, 0.0, 1000.0, 800.0), true);
        assert_eq!(w.ca_transaction_count.load(Ordering::SeqCst), 2);
    }

    /// MockWindow 实现 WindowOps 全部方法：ignores_mouse 读写。
    #[test]
    fn mock_window_ignores_mouse_read_write() {
        let w = MockWindow::new();
        assert!(!w.ignores_mouse(), "默认不忽略鼠标");
        w.set_ignores_mouse(true);
        assert!(w.ignores_mouse());
        w.set_ignores_mouse(false);
        assert!(!w.ignores_mouse());
    }

    /// MockWindow 实现 WindowOps 全部方法：order_out_count 与 order_front。
    #[test]
    fn mock_window_order_counts() {
        let w = MockWindow::new();
        assert_eq!(w.order_out_count(), 0);
        // order_out_count 只读（真实 orderOut 由外部直接递增 order_out_count 字段）
        w.order_out_count.fetch_add(1, Ordering::SeqCst);
        assert_eq!(w.order_out_count(), 1);
        w.order_front();
        assert_eq!(w.order_front_count.load(Ordering::SeqCst), 1);
    }

    /// MockWindow 实现 WindowOps 全部方法：occlusion_detection 读写。
    #[test]
    fn mock_window_occlusion_detection_read_write() {
        let w = MockWindow::new();
        assert!(w.occlusion_detection(), "默认启用遮挡检测");
        w.set_occlusion_detection(false);
        assert!(!w.occlusion_detection());
        w.set_occlusion_detection(true);
        assert!(w.occlusion_detection());
    }

    /// MockWindow 实现 WindowOps 全部方法：collection_behavior 读写。
    #[test]
    fn mock_window_collection_behavior_read_write() {
        let w = MockWindow::new();
        assert_eq!(w.collection_behavior(), 0, "默认 collectionBehavior 为 0");
        w.set_collection_behavior(0x0004); // NSWindowCollectionBehaviorTransient
        assert_eq!(w.collection_behavior(), 0x0004);
        // 位或操作
        let cb = w.collection_behavior();
        w.set_collection_behavior(cb | 0x0008);
        assert_eq!(w.collection_behavior(), 0x000C);
    }

    /// MockWindow 实现 WindowOps 全部方法：corner_radius 读写。
    #[test]
    fn mock_window_corner_radius_read_write() {
        let w = MockWindow::new();
        assert_eq!(w.content_view_corner_radius(), 16.0, "默认圆角 16.0");
        w.set_content_view_corner_radius(12.0);
        assert_eq!(w.content_view_corner_radius(), 12.0);
    }

    /// MockWindow 实现 WindowOps 全部方法：masks_to_bounds 读写。
    #[test]
    fn mock_window_masks_to_bounds_read_write() {
        let w = MockWindow::new();
        assert!(w.content_view_masks_to_bounds(), "默认 masksToBounds=true");
        w.set_content_view_masks_to_bounds(false);
        assert!(!w.content_view_masks_to_bounds());
        w.set_content_view_masks_to_bounds(true);
        assert!(w.content_view_masks_to_bounds());
    }

    /// MockWindow 实现 WindowOps 全部方法：wkwebview_frame 读写。
    #[test]
    fn mock_window_wkwebview_frame_read_write() {
        let w = MockWindow::new();
        let f = Frame::new(0.0, 0.0, 1440.0, 900.0);
        w.set_wkwebview_frame(f);
        assert_eq!(w.wkwebview_frame(), f);
    }

    /// MockWindow 实现 WindowOps 全部方法：observer_count 读写。
    #[test]
    fn mock_window_observer_count_read_write() {
        let w = MockWindow::new();
        assert_eq!(w.observer_count(), 0, "默认 observer_count=0");
        w.observer_count.fetch_add(1, Ordering::SeqCst);
        assert_eq!(w.observer_count(), 1);
        w.observer_count.fetch_sub(1, Ordering::SeqCst);
        assert_eq!(w.observer_count(), 0);
    }

    // ── MockPresentationBridge 基础功能测试 ─────────────────────────────────

    /// MockPresentationBridge::new：SPI 可用，paint 到达，schedule 返回 true。
    #[test]
    fn mock_bridge_available_paint_arrives() {
        let bridge = MockPresentationBridge::new(true, 0);
        let (tx, rx) = std::sync::mpsc::channel::<bool>();
        let ok = bridge.schedule(80, Box::new(move |v| { let _ = tx.send(v); }));
        assert!(ok, "SPI 可用时 schedule 应返回 true");
        let result = rx.recv_timeout(Duration::from_millis(200)).unwrap();
        assert!(result, "paint_will_arrive=true 时回调应以 ok=true 触发");
        assert_eq!(bridge.schedule_count(), 1);
    }

    /// MockPresentationBridge::new：SPI 可用，paint 不到达（超时），回调以 ok=false 触发。
    #[test]
    fn mock_bridge_available_paint_timeout() {
        let bridge = MockPresentationBridge::new(false, 0);
        let (tx, rx) = std::sync::mpsc::channel::<bool>();
        let ok = bridge.schedule(80, Box::new(move |v| { let _ = tx.send(v); }));
        assert!(ok);
        let result = rx.recv_timeout(Duration::from_millis(200)).unwrap();
        assert!(!result, "paint_will_arrive=false 时回调应以 ok=false 触发");
    }

    /// MockPresentationBridge::unavailable：SPI 不可用，schedule 返回 false，不触发回调。
    #[test]
    fn mock_bridge_unavailable_returns_false() {
        let bridge = MockPresentationBridge::unavailable();
        let called = std::sync::Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();
        let ok = bridge.schedule(80, Box::new(move |_| {
            called_clone.store(true, Ordering::SeqCst);
        }));
        assert!(!ok, "SPI 不可用时 schedule 应返回 false");
        // 等待一小段时间确认回调未被触发
        std::thread::sleep(Duration::from_millis(50));
        assert!(!called.load(Ordering::SeqCst), "SPI 不可用时不应触发回调");
        assert_eq!(bridge.schedule_count(), 1, "即使不可用也应计数");
    }

    /// MockPresentationBridge 带延迟：回调在 delay_ms 后触发。
    #[test]
    fn mock_bridge_with_delay() {
        let delay_ms = 30u64;
        let bridge = MockPresentationBridge::new(true, delay_ms);
        let (tx, rx) = std::sync::mpsc::channel::<bool>();
        let start = Instant::now();
        bridge.schedule(80, Box::new(move |v| { let _ = tx.send(v); }));
        let result = rx.recv_timeout(Duration::from_millis(500)).unwrap();
        let elapsed = start.elapsed();
        assert!(result);
        assert!(
            elapsed.as_millis() >= delay_ms as u128,
            "回调应在 delay_ms={} 后触发，实际 elapsed={:?}",
            delay_ms,
            elapsed
        );
    }

    // ── Frame 辅助方法测试 ───────────────────────────────────────────────────

    /// Frame::contains_size 按分量比较。
    #[test]
    fn frame_contains_size() {
        let f = Frame::new(0.0, 0.0, 720.0, 480.0);
        assert!(f.contains_size(720.0, 480.0), "等于时应返回 true");
        assert!(f.contains_size(100.0, 100.0), "小于时应返回 true");
        assert!(!f.contains_size(721.0, 480.0), "宽超出时应返回 false");
        assert!(!f.contains_size(720.0, 481.0), "高超出时应返回 false");
        assert!(!f.contains_size(721.0, 481.0), "宽高均超出时应返回 false");
    }

    /// Frame::default 返回零值 frame。
    #[test]
    fn frame_default_is_zero() {
        let f = Frame::default();
        assert_eq!(f.x, 0.0);
        assert_eq!(f.y, 0.0);
        assert_eq!(f.width, 0.0);
        assert_eq!(f.height, 0.0);
    }

    /// Arc<MockWindow> 可以在多线程间共享（Send + Sync）。
    #[test]
    fn mock_window_is_send_sync() {
        let w = MockWindow::new();
        let w2 = w.clone();
        let handle = std::thread::spawn(move || {
            w2.set_alpha(0.5);
        });
        handle.join().unwrap();
        assert_eq!(w.alpha(), 0.5, "跨线程写入应可见");
    }
}
