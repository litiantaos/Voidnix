//! 主窗口 macOS 原生操作（NSWindow / NSOpenPanel）。platform 层纯原语，
//! runtime/window.rs 负责编排（可见性状态 / click_monitor / focus 还原）。
//!
//! NonactivatingPanel + LSUIElement 模式下,orderFrontRegardless + makeKeyWindow
//! 让面板取得键盘焦点,而不抢 NSApp active —— 原前台应用的菜单栏 / Dock 高亮
//! 全程不变,视觉上像浮层从未离开过当前应用。

use objc2_app_kit::NSWindow;

/// orderFrontRegardless + 浮层 level + 鼠标 hit-test 接管（不 activate NSApp）。
pub fn bring_to_front(window: &tauri::WebviewWindow) {
    if let Ok(raw) = window.ns_window() {
        let raw = raw.cast::<NSWindow>();
        if let Some(ns_window) = unsafe { raw.as_ref() } {
            // 高于普通文档窗，保证叠在原前台 app 之上参与 hit-test
            ns_window.setLevel(objc2_app_kit::NSFloatingWindowLevel);
            ns_window.orderFrontRegardless();
            capture_mouse_events(ns_window);
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
    // 高度动画后 event shape 必须对齐目标尺寸，否则仍按旧矩形/alpha 穿透
    let window_number: objc2_foundation::NSInteger =
        unsafe { objc2::msg_send![ns_window, windowNumber] };
    crate::platform::skylight::set_full_event_shape(window_number as i64, w, h);
}

/// snap-panel 进出场目标（宽高应与稳态一致，只改 origin / alpha，避免 reflow）。
pub struct PanelAnimTarget {
    pub alpha: f64,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub duration: f64,
    /// `true` = easeOut（进场），`false` = easeIn（离场）
    pub ease_out: bool,
}

/// 单 group 同步 **alpha + 纵向位移**（系统曲线，尺寸不变）。
pub fn animate_panel(window: &tauri::WebviewWindow, target: PanelAnimTarget) {
    use objc2_app_kit::NSAnimationContext;
    use objc2_foundation::{ns_string, NSPoint, NSRect, NSSize};
    use objc2_quartz_core::CAMediaTimingFunction;

    let Ok(ptr) = window.ns_window() else {
        return;
    };
    let raw = ptr.cast::<NSWindow>();
    let Some(ns_window) = (unsafe { raw.as_ref() }) else {
        return;
    };
    let frame = NSRect::new(
        NSPoint::new(target.x, target.y),
        NSSize::new(target.w, target.h),
    );
    let timing = CAMediaTimingFunction::functionWithName(if target.ease_out {
        ns_string!("easeOut")
    } else {
        ns_string!("easeIn")
    });

    NSAnimationContext::beginGrouping();
    let ctx = NSAnimationContext::currentContext();
    ctx.setDuration(target.duration);
    ctx.setTimingFunction(Some(&timing));
    unsafe {
        let animator: *mut objc2::runtime::AnyObject = objc2::msg_send![ns_window, animator];
        let _: () = objc2::msg_send![animator, setAlphaValue: target.alpha];
        let _: () = objc2::msg_send![animator, setFrame: frame, display: true];
    }
    NSAnimationContext::endGrouping();
}

/// Mica 材质底：NSVisualEffectView（material=HeaderView，blendingMode=BehindWindow）
/// 作为 contentView 最底层子视图（WKWebView 之下），系统 GPU 合成强实时高斯模糊，
/// 再叠前端 `mica-tint` 白染，得到白色磨砂而非「纯模糊透壁纸」。
/// 强制 aqua appearance 锁浅色（项目仅浅色色阶）。
///
/// 同时配置：窗口本体透明（setOpaque:NO + clearColor）+ contentView 圆角裁剪（CALayer
/// cornerRadius + masksToBounds，含 NSVisualEffectView）+ 子视图 layer 非透明（Tauri
/// transparent:true 只让 WKWebView canvas 透明，CALayer 默认仍 opaque 会盖住材质）。
/// 鼠标穿透由 [`capture_mouse_events`] / SkyLight event shape 处理，不在此层。
///
/// corner_radius 经 contentView CALayer 裁剪：主窗口 16（= radius-window）/
/// snap-panel 10（= radius-panel）。
/// 材质：HeaderView 比 Popover 更密、白染更重，花壁纸上色噪更少仍保留实时模糊。
/// 不用 UnderWindowBackground(21)（近不透明、静态染壁纸色）；不用 WindowBackground(12)
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

    // 幂等守卫：contentView 已含 NSVisualEffectView 则更新 material（配方迭代可热生效），
    // 不重复叠加材质层
    unsafe {
        let ve_class = NSVisualEffectView::class();
        for sv in content_view.subviews().iter() {
            let is_kind: bool = objc2::msg_send![&*sv, isKindOfClass: ve_class];
            if is_kind {
                let _: () = objc2::msg_send![&*sv, setMaterial: NSVisualEffectMaterial::HeaderView];
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
    effect.setMaterial(NSVisualEffectMaterial::HeaderView);
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

/// 透明面板强制接管鼠标：禁止 ignore + 收 mouseMoved + 全窗 event shape（不依赖 alpha）。
pub fn capture_mouse_events(ns_window: &NSWindow) {
    ns_window.setIgnoresMouseEvents(false);
    ns_window.setAcceptsMouseMovedEvents(true);
    crate::platform::skylight::set_full_event_shape_for_nswindow(ns_window);
}

/// 主窗口框架级样式：Mica 材质底（apply_mica_material）+ NonactivatingPanel 转换。
/// 在 lib.rs setup 内 bootstrap 之后调用一次。失败静默跳过。
pub fn apply_main_window_style(window: &tauri::WebviewWindow) {
    let Ok(ptr) = window.ns_window() else { return };
    let raw = ptr.cast::<NSWindow>();
    let Some(ns_window) = (unsafe { raw.as_ref() }) else {
        return;
    };
    apply_mica_material(ns_window, 16.0);
    // 原生阴影：单层窗口（窗口＝面板），阴影提供浅色背景下的层次区分
    ns_window.setHasShadow(true);
    crate::platform::panel::convert_to_panel(raw.cast());
    capture_mouse_events(ns_window);
}

/// NSOpenPanel 选项（文件 / 目录 / 多选 / 扩展名过滤）。
#[derive(Clone, Debug)]
pub struct PickOptions {
    pub can_choose_files: bool,
    pub can_choose_directories: bool,
    pub allows_multiple: bool,
    /// 允许的文件扩展名（无点号，如 `"mp4"`）；空 = 不限制。
    pub allowed_extensions: Vec<String>,
}

impl PickOptions {
    pub fn directory() -> Self {
        Self {
            can_choose_files: false,
            can_choose_directories: true,
            allows_multiple: false,
            allowed_extensions: Vec::new(),
        }
    }

    pub fn files(allows_multiple: bool, allowed_extensions: Vec<String>) -> Self {
        Self {
            can_choose_files: true,
            can_choose_directories: false,
            allows_multiple,
            allowed_extensions,
        }
    }
}

/// NSOpenPanel 模态选择。返回选中路径列表，取消返回空。
/// 调用期间暂停 click-outside 检测；结束后恢复主窗口 key window。
pub fn pick_paths_modal(app: &tauri::AppHandle, opts: PickOptions) -> Vec<String> {
    use objc2::runtime::AnyObject;
    use objc2_foundation::{NSArray, NSString};
    use tauri::Manager;
    unsafe {
        let panel_cls = objc2::class!(NSOpenPanel);
        let panel: *mut AnyObject = objc2::msg_send![panel_cls, openPanel];

        let _: () = objc2::msg_send![panel, setCanChooseFiles: opts.can_choose_files];
        let _: () = objc2::msg_send![panel, setCanChooseDirectories: opts.can_choose_directories];
        let _: () = objc2::msg_send![panel, setAllowsMultipleSelection: opts.allows_multiple];
        if opts.can_choose_directories {
            let _: () = objc2::msg_send![panel, setCanCreateDirectories: true];
        }

        // 扩展名过滤（setAllowedFileTypes，扩展名不含点号）
        if !opts.allowed_extensions.is_empty() {
            let ns_exts: Vec<_> = opts
                .allowed_extensions
                .iter()
                .map(|e| NSString::from_str(e.trim_start_matches('.')))
                .collect();
            let arr = NSArray::from_retained_slice(&ns_exts);
            let _: () = objc2::msg_send![panel, setAllowedFileTypes: &*arr];
        }

        crate::platform::click_monitor::suppress(true);
        // LSUIElement + NonactivatingPanel 下 NSApp 默认 inactive，NSOpenPanel
        // 虽 runModal 但键盘仍留在原前台 app → Esc 关不掉。独占对话框须 activate。
        crate::platform::focus::activate_app();

        // NSModalResponseOK = 1
        let response: isize = objc2::msg_send![panel, runModal];

        crate::platform::click_monitor::suppress(false);
        if let Some(window) = app.get_webview_window("main") {
            make_key_window(&window);
        }

        if response != 1 {
            return Vec::new();
        }

        let urls: *mut AnyObject = objc2::msg_send![panel, URLs];
        let count: usize = objc2::msg_send![urls, count];
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            let url: *mut AnyObject = objc2::msg_send![urls, objectAtIndex: i];
            let path: *mut NSString = objc2::msg_send![url, path];
            if path.is_null() {
                continue;
            }
            out.push((*path).to_string());
        }
        out
    }
}

/// NSOpenPanel 模态选目录。返回选中路径，取消返回空串。
pub fn pick_directory_modal(app: &tauri::AppHandle) -> String {
    pick_paths_modal(app, PickOptions::directory())
        .into_iter()
        .next()
        .unwrap_or_default()
}

/// NSOpenPanel 模态选文件。返回路径列表，取消返回空。
pub fn pick_files_modal(
    app: &tauri::AppHandle,
    allows_multiple: bool,
    allowed_extensions: Vec<String>,
) -> Vec<String> {
    pick_paths_modal(app, PickOptions::files(allows_multiple, allowed_extensions))
}
