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

/// snap-panel 进出场动画：窗口 alpha + frame 同步缩放（单 NSAnimationContext group，
/// CoreAnimation 保证完美同步）。Mica 材质 + 内容整体动画，无 CSS/native 分裂。
/// timing 用 macOS "default" 曲线（标准 ease-in-out），与 animate_frame 一致。
pub fn animate_panel(
    window: &tauri::WebviewWindow,
    target_alpha: f64,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    duration: f64,
) {
    use objc2_foundation::{ns_string, NSPoint, NSRect, NSSize};
    let Ok(ptr) = window.ns_window() else {
        return;
    };
    let raw = ptr.cast::<NSWindow>();
    let Some(ns_window) = (unsafe { raw.as_ref() }) else {
        return;
    };
    let frame = NSRect::new(NSPoint::new(x, y), NSSize::new(w, h));
    unsafe {
        let ctx_cls = objc2::class!(NSAnimationContext);
        let _: () = objc2::msg_send![ctx_cls, beginGrouping];
        let ctx: *mut objc2::runtime::AnyObject = objc2::msg_send![ctx_cls, currentContext];
        let _: () = objc2::msg_send![ctx, setDuration: duration];
        let timing_cls = objc2::class!(CAMediaTimingFunction);
        let timing_fn: *mut objc2::runtime::AnyObject =
            objc2::msg_send![timing_cls, functionWithName: ns_string!("default")];
        let _: () = objc2::msg_send![ctx, setTimingFunction: timing_fn];
        let animator: *mut objc2::runtime::AnyObject = objc2::msg_send![ns_window, animator];
        let _: () = objc2::msg_send![animator, setAlphaValue: target_alpha];
        let _: () = objc2::msg_send![animator, setFrame: frame, display: true];
        let _: () = objc2::msg_send![ctx_cls, endGrouping];
    }
}

/// Mica 材质底：NSVisualEffectView（material=Popover，blendingMode=BehindWindow）
/// 作为 contentView 最底层子视图（WKWebView 之下），系统 GPU 合成强实时高斯模糊，
/// 通透染浅白透出壁纸。强制 aqua appearance 锁浅色（项目仅浅色色阶）。
///
/// 同时配置：窗口本体透明（setOpaque:NO + clearColor）+ contentView 圆角裁剪（CALayer
/// cornerRadius + masksToBounds，含 NSVisualEffectView）+ 子视图 layer 非透明（Tauri
/// transparent:true 只让 WKWebView canvas 透明，CALayer 默认仍 opaque 会盖住材质）。
///
/// corner_radius 经 contentView CALayer 裁剪：主窗口 20（单层，窗口＝面板圆角，无 padding）
/// / snap-panel 12。
/// 注：不用 UnderWindowBackground(21)（近不透明、静态染壁纸色）；不用 WindowBackground(12)
/// （Apple 定性 opaque，无模糊透出）。
pub fn apply_mica_material(ns_window: &NSWindow, corner_radius: f64) {
    use objc2::{ClassType, MainThreadMarker, MainThreadOnly};
    use objc2_app_kit::{
        NSAppearance, NSAppearanceCustomization, NSAutoresizingMaskOptions, NSColor,
        NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState,
        NSVisualEffectView, NSWindowOrderingMode,
    };
    use objc2_foundation::ns_string;

    // 窗口本体透明：setOpaque:NO + clearColor，让 NSVisualEffectView 能透出到 WebView 之下
    ns_window.setOpaque(false);
    ns_window.setBackgroundColor(Some(&NSColor::clearColor()));

    let Some(content_view) = ns_window.contentView() else {
        return;
    };

    // 幂等守卫：contentView 已含 NSVisualEffectView 则跳过，防重复调用叠加材质层
    unsafe {
        let ve_class = NSVisualEffectView::class();
        for sv in content_view.subviews().iter() {
            let is_kind: bool = objc2::msg_send![&*sv, isKindOfClass: ve_class];
            if is_kind {
                return;
            }
        }
    }

    // 圆角裁剪：contentView wantsLayer + cornerRadius + masksToBounds
    // 同时会把矩形 NSVisualEffectView 一并裁出圆角（同层 mask）
    // CALayer 走 msg_send! —— objc2-quartz-core 仅启 CAMediaTimingFunction，未启 CALayer feature
    unsafe {
        let _: () = objc2::msg_send![&content_view, setWantsLayer: true];
        let layer: *mut objc2::runtime::AnyObject = objc2::msg_send![&content_view, layer];
        if !layer.is_null() {
            let _: () = objc2::msg_send![layer, setCornerRadius: corner_radius];
            let _: () = objc2::msg_send![layer, setMasksToBounds: true];
        }
    }

    // 让 contentView 现有子视图（WKWebView）layer 透明：Tauri transparent:true 只设
    // _drawsTransparentBackground（canvas 透明），WKWebView NSView 的 CALayer 默认仍
    // opaque:YES，会整层盖住下层 NSVisualEffectView —— 必须显式遍历置 opaque:NO
    unsafe {
        for sv in content_view.subviews().iter() {
            let _: () = objc2::msg_send![&*sv, setWantsLayer: true];
            let sv_layer: *mut objc2::runtime::AnyObject = objc2::msg_send![&*sv, layer];
            if !sv_layer.is_null() {
                let _: () = objc2::msg_send![sv_layer, setOpaque: false];
            }
        }
    }

    // Mica 材质底：initWithFrame 返回 Retained（自动管理 +1），bounds = contentView 当前尺寸
    // MainThreadMarker::new 返回 Option，调用方保证在主线程（setup / configure_snap_panel），expect 永不触发
    let mtm = MainThreadMarker::new().expect("on main thread");
    let effect =
        NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), content_view.bounds());
    effect.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    effect.setMaterial(NSVisualEffectMaterial::Popover);
    effect.setState(NSVisualEffectState::Active);
    // 锁浅色 appearance：避免跟随系统暗模式导致材质变暗与前端浅色色阶冲突
    if let Some(aqua) = NSAppearance::appearanceNamed(ns_string!("NSAppearanceNameAqua")) {
        effect.setAppearance(Some(&aqua));
    }
    // autoresizing：WidthSizable | HeightSizable，跟随 contentView 尺寸（窗口高度 animator 动画时同步）
    effect.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    // 插到 contentView 最底层（Below），位于 WKWebView 之下
    // addSubview 让 superview 持有 +1，Retained drop 释放 caller 的 +1，引用平衡无泄漏
    content_view.addSubview_positioned_relativeTo(&effect, NSWindowOrderingMode::Below, None);
}

/// 主窗口框架级样式：Mica 材质底（apply_mica_material）+ NonactivatingPanel 转换。
/// 在 lib.rs setup 内 bootstrap 之后调用一次。失败静默跳过。
pub fn apply_main_window_style(window: &tauri::WebviewWindow) {
    let Ok(ptr) = window.ns_window() else { return };
    let raw = ptr.cast::<NSWindow>();
    let Some(ns_window) = (unsafe { raw.as_ref() }) else {
        return;
    };
    apply_mica_material(ns_window, 20.0);
    // 原生阴影：单层窗口（窗口＝面板），阴影提供浅色背景下的层次区分
    ns_window.setHasShadow(true);
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
