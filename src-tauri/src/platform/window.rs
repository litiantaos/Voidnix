//! 主窗口 macOS 原生操作（NSWindow / NSOpenPanel）。platform 层纯原语，
//! runtime/window.rs 负责编排（可见性状态 / click_monitor / focus 还原）。
//!
//! NonactivatingPanel + LSUIElement 模式下,orderFrontRegardless + makeKeyWindow
//! 让面板取得键盘焦点,而不抢 NSApp active —— 原前台应用的菜单栏 / Dock 高亮
//! 全程不变,视觉上像浮层从未离开过当前应用。

use objc2_app_kit::NSWindow;

/// orderFrontRegardless —— 让窗口前置显示但不激活 NSApp。
pub fn bring_to_front(window: &tauri::WebviewWindow) {
    if let Ok(raw) = window.ns_window() {
        let raw = raw.cast::<NSWindow>();
        if let Some(ns_window) = unsafe { raw.as_ref() } {
            ns_window.orderFrontRegardless();
        }
    }
}

/// resignKeyWindow + orderOut —— 隐藏窗口并释放 key window 状态。
pub fn hide_native(window: &tauri::WebviewWindow) {
    if let Ok(raw) = window.ns_window() {
        let raw = raw.cast::<NSWindow>();
        if let Some(ns_window) = unsafe { raw.as_ref() } {
            ns_window.resignKeyWindow();
            ns_window.orderOut(None);
        }
    }
}

/// makeKeyWindow —— 取得键盘焦点（配合 orderFrontRegardless）。
pub fn make_key_window(window: &tauri::WebviewWindow) {
    if let Ok(ptr) = window.ns_window() {
        let raw = ptr.cast::<NSWindow>();
        if let Some(ns_window) = unsafe { raw.as_ref() } {
            ns_window.makeKeyWindow();
        }
    }
}

/// setFrame:display:animate: 经 NSAnimationContext —— 系统级 animator 动画，
/// CoreAnimation 接管插值，不逐帧阻塞主线程、不逐帧触发 WebView 重排（系统合并），
/// 流畅度远超 JS rAF 逐帧 setSize。easeOut 曲线（快速启动 + 平滑收尾）。
/// 坐标转换：Tauri 左上角原点 → NSWindow 左下角原点。
pub fn animate_frame(window: &tauri::WebviewWindow, x: f64, y: f64, w: f64, h: f64) {
    use objc2_foundation::{ns_string, NSPoint, NSRect, NSSize};
    const DURATION_SECS: f64 = 0.26;
    let Ok(ptr) = window.ns_window() else {
        return;
    };
    let raw = ptr.cast::<NSWindow>();
    let Some(ns_window) = (unsafe { raw.as_ref() }) else {
        return;
    };
    unsafe {
        // 窗口当前所在 screen（多屏用窗口自身 screen 而非 mainScreen）
        let screen: *mut objc2::runtime::AnyObject = objc2::msg_send![ns_window, screen];
        if screen.is_null() {
            return;
        }
        let screen_frame: NSRect = objc2::msg_send![screen, frame];
        let ns_y = screen_frame.origin.y + screen_frame.size.height - (y + h);
        let frame = NSRect::new(NSPoint::new(x, ns_y), NSSize::new(w, h));
        // NSAnimationContext group：duration + easeOut timingFunction + animator setFrame
        let ctx_cls = objc2::class!(NSAnimationContext);
        let _: () = objc2::msg_send![ctx_cls, beginGrouping];
        let ctx: *mut objc2::runtime::AnyObject = objc2::msg_send![ctx_cls, currentContext];
        let _: () = objc2::msg_send![ctx, setDuration: DURATION_SECS];
        // timingFunction：苹果系统默认曲线（macOS 窗口动画标准，起步-加速-减速的平滑感）
        let timing_cls = objc2::class!(CAMediaTimingFunction);
        let timing: *mut objc2::runtime::AnyObject =
            objc2::msg_send![timing_cls, functionWithName: ns_string!("default")];
        let _: () = objc2::msg_send![ctx, setTimingFunction: timing];
        let animator: *mut objc2::runtime::AnyObject = objc2::msg_send![ns_window, animator];
        let _: () = objc2::msg_send![animator, setFrame: frame, display: true];
        let _: () = objc2::msg_send![ctx_cls, endGrouping];
    }
}

/// 主窗口框架级样式：content view 圆角（CALayer）+ NonactivatingPanel 转换（§2.8）。
/// 在 lib.rs setup 内 bootstrap 之后调用一次。失败静默跳过（不阻断启动）。
pub fn apply_main_window_style(window: &tauri::WebviewWindow) {
    let Ok(ptr) = window.ns_window() else { return };
    let raw = ptr.cast::<NSWindow>();
    let Some(ns_window) = (unsafe { raw.as_ref() }) else {
        return;
    };
    if let Some(content_view) = ns_window.contentView() {
        let _: () = unsafe { objc2::msg_send![&content_view, setWantsLayer: true] };
        let layer: *mut objc2::runtime::AnyObject =
            unsafe { objc2::msg_send![&content_view, layer] };
        if !layer.is_null() {
            let _: () = unsafe { objc2::msg_send![layer, setCornerRadius: 16.0_f64] };
            let _: () = unsafe { objc2::msg_send![layer, setMasksToBounds: true] };
        }
    }
    crate::platform::panel::convert_to_panel(raw.cast());
}

/// NSOpenPanel 模态选目录。返回选中路径，取消返回空串。
/// 调用期间暂停 click-outside 检测；结束后恢复主窗口 key window。
pub fn pick_directory_modal(app: &tauri::AppHandle) -> String {
    use objc2::runtime::AnyObject;
    use objc2_foundation::NSString;
    use tauri::Manager;
    unsafe {
        let panel_cls = objc2::class!(NSOpenPanel);
        let panel: *mut AnyObject = objc2::msg_send![panel_cls, openPanel];

        // 仅允许选择目录
        let _: () = objc2::msg_send![panel, setCanChooseFiles: false];
        let _: () = objc2::msg_send![panel, setCanChooseDirectories: true];
        let _: () = objc2::msg_send![panel, setAllowsMultipleSelection: false];
        let _: () = objc2::msg_send![panel, setCanCreateDirectories: true];

        // panel 运行期间暂停 click-outside 检测，防止点击 panel 触发窗口隐藏
        crate::platform::click_monitor::suppress(true);

        // 作为独立窗口运行（不附着父窗口，避免 sheet 遮罩）
        // NSModalResponseOK = 1
        let response: isize = objc2::msg_send![panel, runModal];

        crate::platform::click_monitor::suppress(false);
        if let Some(window) = app.get_webview_window("main") {
            make_key_window(&window);
        }

        if response == 1 {
            let urls: *mut AnyObject = objc2::msg_send![panel, URLs];
            let count: usize = objc2::msg_send![urls, count];
            if count > 0 {
                let url: *mut AnyObject = objc2::msg_send![urls, objectAtIndex: 0usize];
                let path: *mut NSString = objc2::msg_send![url, path];
                // M-rs1：path 来自 NSURL.path，对正常 file URL 非空；
                // 但 url 为 nil 或异常路径时可能返 null，解引用前必须检查
                if path.is_null() {
                    return String::new();
                }
                return (*path).to_string();
            }
        }
        String::new()
    }
}
