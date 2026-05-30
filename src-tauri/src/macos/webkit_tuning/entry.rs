#![allow(dead_code)]

use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

#[cfg(target_os = "macos")]
use super::real_window::{RealWindow, RealPresentationBridge};
use super::{toggle, log, throttling, frame_animator, presentation, emoji_warmer};

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

    pub fn contains_size(&self, w: f64, h: f64) -> bool {
        self.width >= w && self.height >= h
    }
}

impl Default for Frame {
    fn default() -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0)
    }
}

pub(crate) trait WindowOps {
    fn alpha(&self) -> f64;
    fn set_alpha(&self, v: f64);

    fn window_frame(&self) -> Frame;
    fn set_window_frame(&self, f: Frame, animated: bool);

    fn ignores_mouse(&self) -> bool;
    fn set_ignores_mouse(&self, v: bool);

    fn order_out_count(&self) -> u32;
    fn order_front(&self);

    fn occlusion_detection(&self) -> bool;
    fn set_occlusion_detection(&self, v: bool);

    fn collection_behavior(&self) -> u64;
    fn set_collection_behavior(&self, v: u64);

    fn content_view_corner_radius(&self) -> f64;
    fn set_content_view_corner_radius(&self, r: f64);

    fn content_view_masks_to_bounds(&self) -> bool;
    fn set_content_view_masks_to_bounds(&self, v: bool);

    fn wkwebview_frame(&self) -> Frame;
    fn set_wkwebview_frame(&self, f: Frame);

    fn observer_count(&self) -> u32;

    fn make_key(&self);
    fn resign_key(&self);
}

pub(crate) trait PresentationBridge: Send + Sync {
    fn schedule(
        &self,
        timeout_ms: u64,
        cb: Box<dyn FnOnce(bool) + Send>,
    ) -> bool;
}

pub fn install(window: &WebviewWindow) -> tauri::Result<()> {
    if window.label() != "main" {
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
            frame_animator::install(&real_window);
            presentation::install();
            emoji_warmer::schedule(window);
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

pub fn make_main_window_key(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    if let Some(window) = app.get_webview_window("main") {
        if let Some(real_window) = RealWindow::from_webview_window(&window) {
            real_window.make_key();
        }
    }
}

pub fn show_main(app: &AppHandle) {
    let mut steps = log::Steps::new();

    let _ = app.emit("showing-window", ());
    let _ = app.emit("webkit-tuning:pre-show", ());
    steps.push("pre-show");

    crate::core::shortcut::set_window_visible(true);

    if !toggle::is_enabled() {
        #[cfg(all(target_os = "macos", not(debug_assertions)))]
        let _ = app.show();
        if let Some(window) = app.get_webview_window("main") {
            #[cfg(target_os = "macos")]
            crate::macos::skylight::move_webview_window_to_active_space(&window);
            let _ = window.show();
        }
        make_main_window_key(app);
        crate::macos::click_monitor::add(app);
        steps.push("legacy-show");
        steps.push("make-key");
        log::event("show", &steps);
        return;
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(window) = app.get_webview_window("main") {
            if let Some(real_window) = RealWindow::from_webview_window(&window) {
                #[cfg(target_os = "macos")]
                crate::macos::skylight::move_webview_window_to_active_space(&window);

                throttling::prepare_show(&real_window, &mut steps);

                let bridge = RealPresentationBridge { window: &window };
                presentation::await_paint(&real_window, &bridge, Some(app), &mut steps);
            } else {
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

    make_main_window_key(app);
    steps.push("make-key");
    crate::macos::click_monitor::add(app);
    log::event("show", &steps);
}

pub fn hide_main(app: &AppHandle) {
    let mut steps = log::Steps::new();

    #[cfg(target_os = "macos")]
    if let Some(window) = app.get_webview_window("main") {
        if let Some(real_window) = RealWindow::from_webview_window(&window) {
            real_window.resign_key();
            steps.push("resign-key");
        }
    }

    if !toggle::is_enabled() {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.hide();
        }
        steps.push("legacy-hide");
    crate::macos::click_monitor::remove();
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
                let _ = window.hide();
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

    crate::macos::click_monitor::remove();
    steps.push("click-monitor-remove");
    log::event("hide", &steps);
}

pub fn resize_main(app: &AppHandle, w: f64, h: f64) -> Result<(), String> {
    let mut steps = log::Steps::new();

    #[cfg(target_os = "macos")]
    {
        if let Some(window) = app.get_webview_window("main") {
            if let Some(real_window) = RealWindow::from_webview_window(&window) {
                frame_animator::ensure_capacity(&real_window, w, h, &mut steps);
                frame_animator::animate(&real_window, w, h, &mut steps);
            } else {
                return Err("NSWindow unavailable".into());
            }
        } else {
            return Err("Main window not found".into());
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.set_size(tauri::LogicalSize::new(w, h));
        }
        steps.push("legacy-set-size");
    }

    log::event("resize", &steps);
    Ok(())
}

#[cfg(any(test, feature = "webkit_tuning_mock"))]
pub(crate) fn install_with<W: WindowOps>(window: &W) {
    if !toggle::is_enabled() {
        log::component_status("Tuning_Toggle", log::Status::Disabled, None);
        return;
    }
    throttling::install(window);
    frame_animator::install(window);
    emoji_warmer::schedule_noop();
    presentation::install();
}

#[cfg(test)]
pub(crate) fn uninstall_for_test<W: WindowOps>(window: &W) {
    let _ = window.observer_count();
}

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
    steps.push("pre-show");
    throttling::prepare_show(window, steps);
    presentation::await_paint(window, bridge, None, steps);
    window.make_key();
    steps.push("make-key");
}

#[cfg(any(test, feature = "webkit_tuning_mock"))]
pub(crate) fn hide_main_with<W: WindowOps>(window: &W, steps: &mut log::Steps) {
    window.resign_key();
    steps.push("resign-key");
    if !toggle::is_enabled() {
        steps.push("legacy-hide");
        return;
    }
    throttling::hide(window, steps);
    steps.push("click-monitor-remove");
}

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

/// 拦截 Cmd+Backspace，阻止 WKWebView 返回导航，通过 Tauri 事件通知前端。
#[cfg(target_os = "macos")]
pub fn intercept_cmd_backspace(app: &AppHandle) {
    use once_cell::sync::OnceCell;

    static APP_HANDLE: OnceCell<AppHandle> = OnceCell::new();
    let _ = APP_HANDLE.set(app.clone());

    extern "C" fn callback() {
        if let Some(h) = APP_HANDLE.get() {
            let _ = h.emit("cmd-backspace", ());
        }
    }

    extern "C" {
        fn voidnix_intercept_cmd_backspace(cb: extern "C" fn());
    }

    unsafe { voidnix_intercept_cmd_backspace(callback); }
}
