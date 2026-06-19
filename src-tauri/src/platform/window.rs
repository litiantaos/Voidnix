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
        unsafe {
            let ns_window = raw.cast::<NSWindow>().as_ref().unwrap();
            ns_window.orderFrontRegardless();
        }
    }
}

/// resignKeyWindow + orderOut —— 隐藏窗口并释放 key window 状态。
pub fn hide_native(window: &tauri::WebviewWindow) {
    if let Ok(raw) = window.ns_window() {
        unsafe {
            let ns_window = raw.cast::<NSWindow>().as_ref().unwrap();
            ns_window.resignKeyWindow();
            ns_window.orderOut(None);
        }
    }
}

/// makeKeyWindow —— 取得键盘焦点（配合 orderFrontRegardless）。
pub fn make_key_window(window: &tauri::WebviewWindow) {
    let raw = window.ns_window().unwrap().cast::<NSWindow>();
    let ns_window = unsafe { raw.as_ref().unwrap() };
    ns_window.makeKeyWindow();
}

/// 主窗口框架级样式：content view 圆角（CALayer）+ NonactivatingPanel 转换（§2.8）。
/// 在 lib.rs setup 内 bootstrap 之后调用一次。
pub fn apply_main_window_style(window: &tauri::WebviewWindow) {
    let raw = window.ns_window().unwrap().cast::<NSWindow>();
    let ns_window = unsafe { raw.as_ref().unwrap() };
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
                return (*path).to_string();
            }
        }
        String::new()
    }
}
