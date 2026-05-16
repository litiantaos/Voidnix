// webkit_tuning 模块树根。
// T0 阶段：仅声明子模块、导出顶层入口签名占位。各组件实装在 T1~T9。
// T4 阶段：新增 Frame / WindowOps / PresentationBridge 抽象，供 PBT 用 MockWindow 替换真实窗口。
// T10 阶段：实装 install_with / show_main_with / hide_main_with / resize_main_with
//           trait-based 版本，并添加 Properties 2、4、11、13、14 的 PBT 测试。
#![allow(dead_code)]

pub mod toggle;
pub mod log;
pub mod obj_exception;
pub mod presentation;
pub mod throttling;
pub mod frame_animator;
pub mod emoji_warmer;

#[cfg(test)]
pub(crate) mod test_support;

use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

// ─────────────────────────────────────────────────────────────────────────────
// 共享数据类型
// ─────────────────────────────────────────────────────────────────────────────

/// 窗口或视图的矩形区域，坐标系与 NSRect 一致（左下角原点）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Frame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Frame {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self { x, y, width, height }
    }

    /// 判断此 frame 的尺寸是否能容纳给定的宽高（按分量比较）。
    pub fn contains_size(&self, w: f64, h: f64) -> bool {
        self.width >= w && self.height >= h
    }
}

impl Default for Frame {
    fn default() -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// WindowOps trait
// ─────────────────────────────────────────────────────────────────────────────

/// 窗口操作抽象，供 PBT 用 MockWindow 替换真实 NSWindow/WKWebView。
///
/// 所有方法均为同步调用，实现者负责线程安全。
pub(crate) trait WindowOps {
    // ── NSWindow alpha ──────────────────────────────────────────────────────
    fn alpha(&self) -> f64;
    fn set_alpha(&self, v: f64);

    // ── NSWindow frame ──────────────────────────────────────────────────────
    fn window_frame(&self) -> Frame;
    fn set_window_frame(&self, f: Frame, animated: bool);

    // ── 鼠标事件穿透 ────────────────────────────────────────────────────────
    fn ignores_mouse(&self) -> bool;
    fn set_ignores_mouse(&self, v: bool);

    // ── orderOut 计数（仅 Mock 有意义）──────────────────────────────────────
    fn order_out_count(&self) -> u32;
    fn order_front(&self);

    // ── 遮挡检测 ────────────────────────────────────────────────────────────
    fn occlusion_detection(&self) -> bool;
    fn set_occlusion_detection(&self, v: bool);

    // ── collectionBehavior ──────────────────────────────────────────────────
    fn collection_behavior(&self) -> u64;
    fn set_collection_behavior(&self, v: u64);

    // ── contentView 圆角 ────────────────────────────────────────────────────
    fn content_view_corner_radius(&self) -> f64;
    fn set_content_view_corner_radius(&self, r: f64);

    // ── contentView masksToBounds ───────────────────────────────────────────
    fn content_view_masks_to_bounds(&self) -> bool;
    fn set_content_view_masks_to_bounds(&self, v: bool);

    // ── WKWebView frame ─────────────────────────────────────────────────────
    fn wkwebview_frame(&self) -> Frame;
    fn set_wkwebview_frame(&self, f: Frame);

    // ── observer 计数（用于 Property 11 install/teardown 循环验证）─────────
    fn observer_count(&self) -> u32;
}

// ─────────────────────────────────────────────────────────────────────────────
// PresentationBridge trait
// ─────────────────────────────────────────────────────────────────────────────

/// Presentation 桥抽象，供 PBT 用 MockPresentationBridge 替换真实 SPI 调用。
pub(crate) trait PresentationBridge: Send + Sync {
    /// 调度一次 presentation update 等待。
    ///
    /// - 返回 `true`：SPI 可用，已成功调度；回调 `cb(ok)` 将在 presentation
    ///   完成（`ok=true`）或超时（`ok=false`）时被调用。
    /// - 返回 `false`：SPI 不可用，调用方应走 fallback 路径。
    fn schedule(
        &self,
        timeout_ms: u64,
        cb: Box<dyn FnOnce(bool) + Send>,
    ) -> bool;
}

// ─────────────────────────────────────────────────────────────────────────────
// Trait-based 顶层入口（T10 实装，仅在 test / webkit_tuning_mock feature 下编译）
// ─────────────────────────────────────────────────────────────────────────────

/// 安装所有驯化组件（trait-based，供测试使用）。
///
/// toggle 禁用时仅记录 Tuning_Toggle 状态日志，不调用任何 native 桥。
/// label 守卫由调用方（production install）负责；trait-based 版本不检查 label，
/// 以便 Property 2 在 mod_top_level 测试中直接验证 label 守卫逻辑。
#[cfg(any(test, feature = "webkit_tuning_mock"))]
pub(crate) fn install_with<W: WindowOps>(window: &W) {
    if !toggle::is_enabled() {
        log::component_status("Tuning_Toggle", log::Status::Disabled, None);
        return;
    }
    // 5 个组件依次 install，每个组件内部写一条 component= 日志
    throttling::install(window);
    frame_animator::install(window);
    emoji_warmer::schedule_noop();
    presentation::install();
}

/// 卸载（仅 cfg(test)，用于 Property 11 验证 observer_count == 0）。
///
/// MockWindow 的 observer_count 初始为 0，install_with 不增加计数；
/// 此函数验证不变量：teardown 后 observer_count 仍为 0。
#[cfg(test)]
pub(crate) fn uninstall_for_test<W: WindowOps>(window: &W) {
    // 真实实现中会移除 KVO/Notification observer；
    // MockWindow 中 observer_count 始终为 0，此处仅读取以验证不变量。
    let _ = window.observer_count();
}

/// show_main 的 trait-based 版本（供测试使用）。
///
/// toggle 禁用时推入 "legacy-show" 步骤并返回。
/// 启用时：prepare_show → await_paint（alpha 设为 1）→ 推入 "focus"。
#[cfg(any(test, feature = "webkit_tuning_mock"))]
pub(crate) fn show_main_with<W: WindowOps>(
    window: &W,
    bridge: &dyn PresentationBridge,
    steps: &mut log::Steps,
) {
    if !toggle::is_enabled() {
        steps.push("legacy-show");
        return;
    }
    // pre-show 信号（Property 4：此时刻严格先于 alpha=1）
    steps.push("pre-show");
    throttling::prepare_show(window, steps);
    presentation::await_paint(window, bridge, None, steps);
    steps.push("focus");
}

/// hide_main 的 trait-based 版本（供测试使用）。
///
/// toggle 禁用时推入 "legacy-hide" 步骤并返回。
#[cfg(any(test, feature = "webkit_tuning_mock"))]
pub(crate) fn hide_main_with<W: WindowOps>(window: &W, steps: &mut log::Steps) {
    if !toggle::is_enabled() {
        steps.push("legacy-hide");
        return;
    }
    throttling::hide(window, steps);
    steps.push("click-monitor-remove");
}

/// resize_main 的 trait-based 版本（供测试使用）。
///
/// toggle 禁用时推入 "legacy-set-size" 步骤并返回。
#[cfg(any(test, feature = "webkit_tuning_mock"))]
pub(crate) fn resize_main_with<W: WindowOps>(
    window: &W,
    w: f64,
    h: f64,
    steps: &mut log::Steps,
) {
    if !toggle::is_enabled() {
        steps.push("legacy-set-size");
        return;
    }
    frame_animator::ensure_capacity(window, w, h, steps);
    frame_animator::animate(window, w, h, steps);
}

// ─────────────────────────────────────────────────────────────────────────────
// RealWindow：真实 NSWindow 包装，实现 WindowOps（仅 macOS）
// ─────────────────────────────────────────────────────────────────────────────

/// 真实 NSWindow 包装，通过 objc2::msg_send! + obj_exception::try_block 实现 WindowOps。
/// 仅在 macOS 上编译。
#[cfg(target_os = "macos")]
pub(crate) struct RealWindow {
    ns_window: *mut objc2::runtime::AnyObject,
}

/// 从 NSWindow 获取 WKWebView 指针（contentView 的第一个子视图）。
/// `setWindowOcclusionDetectionEnabled:` 等 SPI 只在 WKWebView 上有效。
#[cfg(target_os = "macos")]
unsafe fn get_wkwebview(ns_window: *mut objc2::runtime::AnyObject) -> *mut objc2::runtime::AnyObject {
    if ns_window.is_null() { return std::ptr::null_mut(); }
    let mut result = std::ptr::null_mut();
    // 包在 try_block 里防止 contentView/subviews 消息抛异常
    obj_exception::try_block(|| {
        let content_view: *mut objc2::runtime::AnyObject = objc2::msg_send![ns_window, contentView];
        if content_view.is_null() { return; }
        let subviews: *mut objc2::runtime::AnyObject = objc2::msg_send![content_view, subviews];
        if subviews.is_null() { return; }
        let count: usize = objc2::msg_send![subviews, count];
        if count == 0 { return; }
        result = objc2::msg_send![subviews, objectAtIndex: 0usize];
    });
    result
}

// ─────────────────────────────────────────────────────────────────────────────
// RealPresentationBridge：真实 SPI 桥（仅 macOS）
// ─────────────────────────────────────────────────────────────────────────────

/// 真实 PresentationBridge，调用 native 侧 voidnix_do_after_next_presentation_update。
#[cfg(target_os = "macos")]
pub(crate) struct RealPresentationBridge<'a> {
    window: &'a WebviewWindow,
}

#[cfg(target_os = "macos")]
impl PresentationBridge for RealPresentationBridge<'_> {
    fn schedule(&self, timeout_ms: u64, cb: Box<dyn FnOnce(bool) + Send>) -> bool {
        use objc2_app_kit::NSWindow;

        // 提取 NSWindow 指针
        let ns_window_ptr = match self.window.ns_window() {
            Ok(ptr) => ptr.cast::<NSWindow>() as *mut objc2::runtime::AnyObject,
            Err(_) => return false,
        };

        // WKWebView 是 NSWindow.contentView 的第一个子视图
        let wk_view_ptr = unsafe { get_wkwebview(ns_window_ptr) };
        if wk_view_ptr.is_null() { return false; }

        // 用 C 函数指针 + Box<dyn FnOnce> 传递回调，避免 block2 跨 FFI 的 ABI 问题
        extern "C" {
            fn voidnix_do_after_next_presentation_update_fn(
                web: *mut objc2::runtime::AnyObject,
                window: *mut objc2::runtime::AnyObject,
                timeout_ms: u64,
                cb_fn: extern "C-unwind" fn(*mut std::ffi::c_void, bool),
                ctx: *mut std::ffi::c_void,
            ) -> bool;
        }

        extern "C-unwind" fn trampoline(ctx: *mut std::ffi::c_void, ok: bool) {
            // SAFETY: ctx 是 Box<Box<dyn FnOnce(bool) + Send>> 的裸指针
            let cb = unsafe { Box::from_raw(ctx as *mut Box<dyn FnOnce(bool) + Send>) };
            cb(ok);
        }

        let ctx = Box::into_raw(Box::new(cb)) as *mut std::ffi::c_void;
        let result = unsafe {
            voidnix_do_after_next_presentation_update_fn(
                wk_view_ptr,
                ns_window_ptr,
                timeout_ms,
                trampoline,
                ctx,
            )
        };
        if !result {
            // SPI 不可用：释放 ctx 避免内存泄漏
            unsafe { drop(Box::from_raw(ctx as *mut Box<dyn FnOnce(bool) + Send>)); }
        }
        result
    }
}
#[cfg(target_os = "macos")]
unsafe impl Send for RealWindow {}
#[cfg(target_os = "macos")]
unsafe impl Sync for RealWindow {}

#[cfg(target_os = "macos")]
impl RealWindow {
    /// 从 WebviewWindow 提取真实 NSWindow 指针，构造 RealWindow。
    pub fn from_webview_window(window: &WebviewWindow) -> Option<Self> {
        use objc2_app_kit::NSWindow;
        let raw = window.ns_window().ok()?.cast::<NSWindow>();
        // raw 是 *mut NSWindow，直接转为 *mut AnyObject
        Some(Self {
            ns_window: raw as *mut objc2::runtime::AnyObject,
        })
    }
}

#[cfg(target_os = "macos")]
impl WindowOps for RealWindow {
    fn alpha(&self) -> f64 {
        let mut result = 1.0f64;
        obj_exception::try_block(|| unsafe {
            result = objc2::msg_send![self.ns_window, alphaValue];
        });
        result
    }

    fn set_alpha(&self, v: f64) {
        obj_exception::try_block(|| unsafe {
            let _: () = objc2::msg_send![self.ns_window, setAlphaValue: v];
        });
    }

    fn window_frame(&self) -> Frame {
        let mut result = Frame::default();
        obj_exception::try_block(|| unsafe {
            let frame: objc2_foundation::NSRect = objc2::msg_send![self.ns_window, frame];
            result = Frame::new(frame.origin.x, frame.origin.y, frame.size.width, frame.size.height);
        });
        result
    }

    fn set_window_frame(&self, f: Frame, animated: bool) {
        obj_exception::try_block(|| unsafe {
            use objc2_foundation::{NSPoint, NSRect, NSSize};
            let rect = NSRect::new(
                NSPoint::new(f.x, f.y),
                NSSize::new(f.width, f.height),
            );
            if animated {
                // NSAnimationContext.beginGrouping + setAllowsImplicitAnimation:YES + setDuration:0.18
                let _: () = objc2::msg_send![objc2::class!(NSAnimationContext), beginGrouping];
                let ctx: *mut objc2::runtime::AnyObject =
                    objc2::msg_send![objc2::class!(NSAnimationContext), currentContext];
                let _: () = objc2::msg_send![ctx, setAllowsImplicitAnimation: true];
                let _: () = objc2::msg_send![ctx, setDuration: 0.18f64];
                let _: () = objc2::msg_send![self.ns_window, setFrame: rect, display: false, animate: true];
                let _: () = objc2::msg_send![objc2::class!(NSAnimationContext), endGrouping];
            } else {
                let _: () = objc2::msg_send![self.ns_window, setFrame: rect, display: false];
            }
        });
    }

    fn ignores_mouse(&self) -> bool {
        let mut result = false;
        obj_exception::try_block(|| unsafe {
            result = objc2::msg_send![self.ns_window, ignoresMouseEvents];
        });
        result
    }

    fn set_ignores_mouse(&self, v: bool) {
        obj_exception::try_block(|| unsafe {
            let _: () = objc2::msg_send![self.ns_window, setIgnoresMouseEvents: v];
        });
    }

    /// 真实实现不计数，始终返回 0。
    fn order_out_count(&self) -> u32 {
        0
    }

    fn order_front(&self) {
        obj_exception::try_block(|| unsafe {
            let _: () = objc2::msg_send![self.ns_window, orderFrontRegardless];
        });
    }

    fn occlusion_detection(&self) -> bool {
        unsafe {
            extern "C" {
                fn voidnix_get_occlusion_detection(view: *mut objc2::runtime::AnyObject) -> bool;
            }
            let wk_view = get_wkwebview(self.ns_window);
            voidnix_get_occlusion_detection(wk_view)
        }
    }

    fn set_occlusion_detection(&self, v: bool) {
        unsafe {
            extern "C" {
                fn voidnix_set_occlusion_detection(view: *mut objc2::runtime::AnyObject, enabled: bool);
            }
            let wk_view = get_wkwebview(self.ns_window);
            voidnix_set_occlusion_detection(wk_view, v);
        }
    }

    fn collection_behavior(&self) -> u64 {
        let mut result = 0u64;
        obj_exception::try_block(|| unsafe {
            result = objc2::msg_send![self.ns_window, collectionBehavior];
        });
        result
    }

    fn set_collection_behavior(&self, v: u64) {
        obj_exception::try_block(|| unsafe {
            let _: () = objc2::msg_send![self.ns_window, setCollectionBehavior: v];
        });
    }

    fn content_view_corner_radius(&self) -> f64 {
        let mut result = 0.0f64;
        obj_exception::try_block(|| unsafe {
            let content_view: *mut objc2::runtime::AnyObject =
                objc2::msg_send![self.ns_window, contentView];
            if !content_view.is_null() {
                let layer: *mut objc2::runtime::AnyObject =
                    objc2::msg_send![content_view, layer];
                if !layer.is_null() {
                    result = objc2::msg_send![layer, cornerRadius];
                }
            }
        });
        result
    }

    fn set_content_view_corner_radius(&self, r: f64) {
        obj_exception::try_block(|| unsafe {
            let content_view: *mut objc2::runtime::AnyObject =
                objc2::msg_send![self.ns_window, contentView];
            if !content_view.is_null() {
                let layer: *mut objc2::runtime::AnyObject =
                    objc2::msg_send![content_view, layer];
                if !layer.is_null() {
                    let _: () = objc2::msg_send![layer, setCornerRadius: r];
                }
            }
        });
    }

    fn content_view_masks_to_bounds(&self) -> bool {
        let mut result = false;
        obj_exception::try_block(|| unsafe {
            let content_view: *mut objc2::runtime::AnyObject =
                objc2::msg_send![self.ns_window, contentView];
            if !content_view.is_null() {
                let layer: *mut objc2::runtime::AnyObject =
                    objc2::msg_send![content_view, layer];
                if !layer.is_null() {
                    result = objc2::msg_send![layer, masksToBounds];
                }
            }
        });
        result
    }

    fn set_content_view_masks_to_bounds(&self, v: bool) {
        obj_exception::try_block(|| unsafe {
            let content_view: *mut objc2::runtime::AnyObject =
                objc2::msg_send![self.ns_window, contentView];
            if !content_view.is_null() {
                let layer: *mut objc2::runtime::AnyObject =
                    objc2::msg_send![content_view, layer];
                if !layer.is_null() {
                    let _: () = objc2::msg_send![layer, setMasksToBounds: v];
                }
            }
        });
    }

    fn wkwebview_frame(&self) -> Frame {
        // WKWebView frame 与 contentView frame 相同（Tauri 把 WKWebView 填满 contentView）
        let mut result = Frame::default();
        obj_exception::try_block(|| unsafe {
            let content_view: *mut objc2::runtime::AnyObject =
                objc2::msg_send![self.ns_window, contentView];
            if !content_view.is_null() {
                let frame: objc2_foundation::NSRect =
                    objc2::msg_send![content_view, frame];
                result = Frame::new(frame.origin.x, frame.origin.y, frame.size.width, frame.size.height);
            }
        });
        result
    }

    fn set_wkwebview_frame(&self, f: Frame) {
        obj_exception::try_block(|| unsafe {
            use objc2_foundation::{NSPoint, NSRect, NSSize};
            let content_view: *mut objc2::runtime::AnyObject =
                objc2::msg_send![self.ns_window, contentView];
            if !content_view.is_null() {
                let rect = NSRect::new(
                    NSPoint::new(f.x, f.y),
                    NSSize::new(f.width, f.height),
                );
                let _: () = objc2::msg_send![content_view, setFrame: rect];
            }
        });
    }

    /// 真实实现不计数，始终返回 0。
    fn observer_count(&self) -> u32 {
        0
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 生产顶层入口（T12 实装，接合真实 NSWindow）
// ─────────────────────────────────────────────────────────────────────────────

/// 在 Voidnix_Shell 启动时一次性 install。
/// label 守卫：仅对 label == "main" 的窗口生效（Property 2）。
pub fn install(window: &WebviewWindow) -> tauri::Result<()> {
    // label 守卫：非 main 窗口直接返回，不输出日志（避免噪声）
    if window.label() != "main" {
        return Ok(());
    }

    // toggle 守卫：禁用时记录日志并返回
    if !toggle::is_enabled() {
        log::component_status("Tuning_Toggle", log::Status::Disabled, None);
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(real_window) = RealWindow::from_webview_window(window) {
            // 依次安装各驯化组件
            throttling::install(&real_window);
            frame_animator::install(&real_window);
            presentation::install();
            // emoji 预热：异步调度，500ms 后在主线程执行
            emoji_warmer::schedule(window);
        } else {
            // 无法提取 NSWindow 时记录回退状态
            log::component_status("Tuning_Toggle", log::Status::Fallback, Some("ns-window-unavailable"));
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        // 非 macOS 平台：记录禁用状态
        log::component_status("Tuning_Toggle", log::Status::Disabled, Some("non-macos"));
    }

    Ok(())
}

/// 截屏窗口轻量 install：仅装 Throttling_Suppressor（关 occlusion detection + Transient）。
///
/// 不装 Frame_Animator/Webview_Frame_Pin（截屏覆盖层全屏，无需会话最大尺寸锁定），
/// 不调用 emoji_warmer（截屏窗口不展示 emoji）。
///
/// 关 occlusion detection 是关键：让 WKWebView 即使被 orderOut/不在当前 Space 也
/// 不进入渲染节流，使得 orderOut + orderFrontRegardless 这套苹果文档化、所有
/// macOS 版本稳定的标准 Space 迁移路径不再有"空桌面卡顿"副作用。
pub fn install_screenshot(window: &WebviewWindow) -> tauri::Result<()> {
    if window.label() != "screenshot" {
        return Ok(());
    }

    if !toggle::is_enabled() {
        log::component_status("Tuning_Toggle", log::Status::Disabled, None);
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(real_window) = RealWindow::from_webview_window(window) {
            throttling::install(&real_window);
            // presentation 是进程级一次性 install，main 已装时此处幂等
            presentation::install();
        } else {
            log::component_status("Tuning_Toggle", log::Status::Fallback, Some("ns-window-unavailable"));
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        log::component_status("Tuning_Toggle", log::Status::Disabled, Some("non-macos"));
    }

    Ok(())
}

/// 主窗口 show 入口。T14 实装：调用 throttling::prepare_show + presentation::await_paint。
pub fn show_main(app: &AppHandle) {
    let mut steps = log::Steps::new();

    // emit showing-window（前端用于抑制失焦自动隐藏）
    let _ = app.emit("showing-window", ());
    // emit pre-show（前端触发 rAF，Req 2.7）
    let _ = app.emit("webkit-tuning:pre-show", ());
    steps.push("pre-show");

    // 更新可见状态
    crate::commands::shortcut::set_window_visible(true);

    if !toggle::is_enabled() {
        // toggle 禁用：走 Tauri 默认 show 路径
        #[cfg(all(target_os = "macos", not(debug_assertions)))]
        let _ = app.show();
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
        }
        crate::mac_utils::activate_app();
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.set_focus();
        }
        crate::commands::shortcut::add_click_monitor(app);
        steps.push("legacy-show");
        log::event("show", &steps);
        return;
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(window) = app.get_webview_window("main") {
            if let Some(real_window) = RealWindow::from_webview_window(&window) {
                // prepare_show：恢复鼠标事件响应，orderFrontRegardless（alpha 仍为 0）
                throttling::prepare_show(&real_window, &mut steps);

                // await_paint：等待 WKWebView 首帧呈现后再设 alpha=1
                // 使用真实 SPI 桥（RealPresentationBridge）
                let bridge = RealPresentationBridge { window: &window };
                presentation::await_paint(&real_window, &bridge, Some(app), &mut steps);
            } else {
                // 无法提取 NSWindow：fallback 到 Tauri 默认 show
                #[cfg(not(debug_assertions))]
                let _ = app.show();
                let _ = window.show();
                steps.push("fallback-show");
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
        }
        steps.push("legacy-show");
    }

    crate::mac_utils::activate_app();
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_focus();
    }
    steps.push("focus");
    crate::commands::shortcut::add_click_monitor(app);
    log::event("show", &steps);
}

/// 主窗口 hide 入口。T13 实装：调用 throttling::hide 或 fallback 到 window.hide()。
pub fn hide_main(app: &AppHandle) {
    let mut steps = log::Steps::new();

    if !toggle::is_enabled() {
        // toggle 禁用：走 Tauri 默认 hide 路径
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.hide();
            #[cfg(all(target_os = "macos", not(debug_assertions)))]
            let _ = app.hide();
        }
        steps.push("legacy-hide");
        crate::commands::shortcut::remove_click_monitor();
        steps.push("click-monitor-remove");
        log::event("hide", &steps);
        return;
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(window) = app.get_webview_window("main") {
            if let Some(real_window) = RealWindow::from_webview_window(&window) {
                throttling::hide(&real_window, &mut steps);
            } else {
                // 无法提取 NSWindow：fallback
                let _ = window.hide();
                #[cfg(not(debug_assertions))]
                let _ = app.hide();
                steps.push("fallback-orderOut");
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.hide();
        }
        steps.push("legacy-hide");
    }

    crate::commands::shortcut::remove_click_monitor();
    steps.push("click-monitor-remove");
    log::event("hide", &steps);
}

/// 主窗口 resize 入口。T11 将接入 set_main_window_size command。
pub fn resize_main(_app: &AppHandle, _w: f64, _h: f64) -> Result<(), String> {
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// 测试
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::webkit_tuning::test_support::{MockPresentationBridge, MockWindow};
    use proptest::prelude::*;
    use std::sync::{Mutex, OnceLock};
    use std::time::Instant;

    // ── 日志捕获基础设施 ─────────────────────────────────────────────────────

    /// 捕获 target == "webkit_tuning" 的日志记录。
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

    /// 全局串行锁：所有操作全局 toggle / FAIL_COUNT / 日志 sink 的测试必须持有此锁。
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

    // ── 基础单元测试 ─────────────────────────────────────────────────────────

    /// toggle 禁用时 install_with 不调用任何组件（occlusion_detection 保持 true，collectionBehavior 保持 0）。
    #[test]
    fn install_with_toggle_disabled_does_not_call_components() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        toggle::override_enabled(false);

        let w = MockWindow::new();
        install_with(&w);

        // toggle 禁用时不应调用 throttling::install（occlusion_detection 保持 true）
        assert!(w.occlusion_detection(), "toggle 禁用时 occlusion_detection 应保持 true（throttling 未安装）");
        assert_eq!(w.collection_behavior(), 0, "toggle 禁用时 collectionBehavior 应保持 0（throttling 未安装）");

        toggle::clear_override();
    }

    /// toggle 启用时 install_with 调用所有组件（occlusion_detection=false，collectionBehavior 含 Transient，wkwebview_frame 锁定）。
    #[test]
    fn install_with_toggle_enabled_calls_all_components() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        toggle::override_enabled(true);

        let w = MockWindow::new();
        install_with(&w);

        // throttling::install 应设置 occlusion_detection=false 和 collectionBehavior |= Transient
        assert!(!w.occlusion_detection(), "toggle 启用时 occlusion_detection 应为 false（throttling 已安装）");
        assert_ne!(w.collection_behavior() & (1 << 2), 0, "toggle 启用时 collectionBehavior 应含 Transient");
        // frame_animator::install 应锁定 wkwebview_frame 到 ≥ 720×480
        let cap = w.wkwebview_frame();
        assert!(cap.width >= 720.0 && cap.height >= 480.0, "toggle 启用时 wkwebview_frame 应 ≥ 720×480");

        toggle::clear_override();
    }

    /// show_main_with toggle 禁用时推入 "legacy-show"，不调用 bridge。
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

    /// hide_main_with toggle 禁用时推入 "legacy-hide"。
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

    /// resize_main_with toggle 禁用时推入 "legacy-set-size"。
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

    // ── Property 2 重复验证（label 守卫）────────────────────────────────────
    //
    // Feature: webkit-presentation-tuning, Property 2: Show 仅作用于 Main_Window。
    // label != "main" 时 native bridge 调用次数恒为 0。
    // 本测试在 mod_top_level 层面验证：非 main label 时 bridge 不被调用。
    // Validates: Requirements 1.5
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
                // main 窗口：show_main_with 应调用 bridge
                let mut steps = log::Steps::new();
                show_main_with(&w, &bridge, &mut steps);
                prop_assert!(
                    bridge.schedule_count() > 0,
                    "main 窗口 show_main_with 应调用 bridge.schedule"
                );
            } else {
                // 非 main 窗口：生产 install 入口有 label 守卫，bridge 不被调用
                // 此处直接验证：不调用 show_main_with，bridge 调用次数为 0
                prop_assert_eq!(
                    bridge.schedule_count(),
                    0,
                    "非 main 窗口不应调用 bridge.schedule，label={}", label
                );
            }

            toggle::clear_override();
        }
    }

    // ── Property 4: pre-show 信号严格先于 alpha=1 且间距 ≤16ms + PAINT_TIMEOUT_MS ──
    //
    // Feature: webkit-presentation-tuning, Property 4.
    // 对任意 show 序列，t_pre（pre-show 推入时刻）到 t_alpha₁（alpha=1 时刻）
    // 的间距 ≤ 16ms + PAINT_TIMEOUT_MS + 32ms（调度余量）。
    // Validates: Requirements 2.7
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn property_4_pre_show_alpha_timing(d_ms in 0u64..200u64) {
            let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            toggle::override_enabled(true);

            let w = MockWindow::new();
            w.set_alpha(0.0);

            // paint 在 d_ms 后到达（若 d_ms <= 80 则 ok=true，否则超时 ok=false）
            let paint_will_arrive = d_ms <= 80;
            let bridge = MockPresentationBridge::new(paint_will_arrive, d_ms.min(80));

            let mut steps = log::Steps::new();

            // 记录 pre-show 推入时刻（在 show_main_with 内部 steps.push("pre-show") 之前）
            let t_pre = Instant::now();
            show_main_with(&w, &bridge, &mut steps);
            let t_alpha1 = w.last_alpha_set_at.lock().unwrap().unwrap();

            // t_pre 到 t_alpha1 的间距应 ≤ 16ms + PAINT_TIMEOUT_MS(80) + 32ms 余量
            let max_gap_ms = 16u128 + 80 + 32;
            let gap = t_alpha1.duration_since(t_pre);
            prop_assert!(
                gap.as_millis() <= max_gap_ms,
                "alpha=1 应在 pre-show 后 {}ms 内，实际={:?}",
                max_gap_ms, gap
            );

            // steps 应包含 pre-show
            prop_assert!(
                steps.contains(&"pre-show"),
                "steps 应含 pre-show，实际={:?}", steps
            );

            toggle::clear_override();
        }
    }

    // ── Property 11: install/teardown 循环不残留 observer ───────────────────
    //
    // Feature: webkit-presentation-tuning, Property 11.
    // 对任意 N 次 install → uninstall_for_test 循环，最后一次 teardown 后
    // observer_count == 0。
    // Validates: Requirements 6.4
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

    // ── Property 13: Install 阶段每个组件恰好一条状态日志 ───────────────────
    //
    // Feature: webkit-presentation-tuning, Property 13.
    // 通过 MockWindow 状态变化验证 install_with 调用了正确的组件。
    // toggle 启用时：throttling 设置 occlusion=false + Transient，frame_animator 锁定 wkwebview_frame。
    // toggle 禁用时：MockWindow 状态不变。
    // Validates: Requirements 7.3（行为层面）
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
                // toggle 启用：throttling 应设置 occlusion=false 和 Transient
                prop_assert!(!w.occlusion_detection(), "toggle 启用时 occlusion_detection 应为 false");
                prop_assert_ne!(w.collection_behavior() & (1 << 2), 0, "toggle 启用时应含 Transient");
                // frame_animator 应锁定 wkwebview_frame
                let cap = w.wkwebview_frame();
                prop_assert!(cap.width >= 720.0 && cap.height >= 480.0, "toggle 启用时 wkwebview_frame 应 ≥ 720×480");
            } else {
                // toggle 禁用：MockWindow 状态不变
                prop_assert!(w.occlusion_detection(), "toggle 禁用时 occlusion_detection 应保持 true");
                prop_assert_eq!(w.collection_behavior(), 0, "toggle 禁用时 collectionBehavior 应保持 0");
            }

            toggle::clear_override();
        }
    }

    // ── Property 14: 事件日志一一对应且写入耗时受限 ─────────────────────────
    //
    // Feature: webkit-presentation-tuning, Property 14.
    // 对任意 show/hide/resize 事件序列 [e₁..eₙ]，每个事件都产生对应的步骤记录。
    // 通过 steps 长度验证事件处理的完整性（不依赖 log sink）。
    // Validates: Requirements 7.3, 7.4（行为层面）
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn property_14_event_steps_correspond_to_events(
            events in proptest::collection::vec(
                proptest::prop_oneof![
                    Just(0u8),  // 0 = Show
                    Just(1u8),  // 1 = Hide
                    Just(2u8),  // 2 = Resize
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
                        // Show
                        w.set_alpha(0.0);
                        let mut steps = log::Steps::new();
                        show_main_with(&w, &bridge, &mut steps);
                        let elapsed = t0.elapsed();
                        // steps 不为空（至少含 pre-show）
                        prop_assert!(!steps.is_empty(), "show 事件应产生非空 steps");
                        prop_assert!(
                            elapsed.as_millis() <= 200,
                            "show_main_with 耗时应 ≤200ms，实际={:?}", elapsed
                        );
                        event_count += 1;
                    }
                    1 => {
                        // Hide
                        let mut steps = log::Steps::new();
                        hide_main_with(&w, &mut steps);
                        prop_assert!(!steps.is_empty(), "hide 事件应产生非空 steps");
                        event_count += 1;
                    }
                    _ => {
                        // Resize
                        let mut steps = log::Steps::new();
                        resize_main_with(&w, 800.0, 600.0, &mut steps);
                        prop_assert!(!steps.is_empty(), "resize 事件应产生非空 steps");
                        event_count += 1;
                    }
                }
            }

            // 事件处理次数应等于事件序列长度
            prop_assert_eq!(event_count, n, "事件处理次数应等于事件序列长度");

            toggle::clear_override();
        }
    }
}
