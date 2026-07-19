//! 主窗口 macOS 原生操作（NSWindow / NSOpenPanel）。platform 层纯原语，
//! runtime/window.rs 负责编排（可见性状态 / click_monitor / focus 还原）。
//!
//! NonactivatingPanel + LSUIElement 模式下,orderFrontRegardless + makeKeyWindow
//! 让面板取得键盘焦点,而不抢 NSApp active —— 原前台应用的菜单栏 / Dock 高亮
//! 全程不变,视觉上像浮层从未离开过当前应用。

use objc2_app_kit::NSWindow;
use objc2_foundation::NSRect;
use std::sync::Mutex;

/// 默认主窗逻辑尺寸（与 tauri.conf / WINDOW 常量一致）。
const MAIN_DEFAULT_W: f64 = 720.0;
const MAIN_DEFAULT_H: f64 = 480.0;

/// show 时锁定的目标屏 visibleFrame（Cocoa）。
#[derive(Clone, Copy, Debug)]
struct PlacementVis {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl PlacementVis {
    fn from_ns(r: NSRect) -> Self {
        Self {
            x: r.origin.x,
            y: r.origin.y,
            w: r.size.width,
            h: r.size.height,
        }
    }
    fn to_ns(self) -> NSRect {
        use objc2_foundation::{NSPoint, NSSize};
        NSRect::new(NSPoint::new(self.x, self.y), NSSize::new(self.w, self.h))
    }
}

static PLACEMENT_VIS: Mutex<Option<PlacementVis>> = Mutex::new(None);

fn store_placement(vis: NSRect) {
    *PLACEMENT_VIS.lock().unwrap_or_else(|e| e.into_inner()) = Some(PlacementVis::from_ns(vis));
}

fn load_placement() -> Option<PlacementVis> {
    *PLACEMENT_VIS.lock().unwrap_or_else(|e| e.into_inner())
}

/// hide 时调用：清掉 show 时锁定的 placement，避免隐藏态仍按旧屏改尺寸。
pub fn cancel_pending_present() {
    *PLACEMENT_VIS.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// orderFrontRegardless + 浮层 level + 鼠标 hit-test 接管（不 activate NSApp）。
pub fn bring_to_front(window: &tauri::WebviewWindow) {
    if let Ok(raw) = window.ns_window() {
        let raw = raw.cast::<NSWindow>();
        if let Some(ns_window) = unsafe { raw.as_ref() } {
            ns_window.setLevel(objc2_app_kit::NSFloatingWindowLevel);
            ns_window.setAlphaValue(1.0);
            ns_window.orderFrontRegardless();
            capture_mouse_events(ns_window);
        }
    }
}

/// 光标所在屏的 visibleFrame（Cocoa）。
fn cursor_visible_frame() -> Option<NSRect> {
    use objc2_app_kit::{NSEvent, NSScreen};
    use objc2_foundation::MainThreadMarker;

    let mtm = MainThreadMarker::new()?;
    let loc = NSEvent::mouseLocation();
    for screen in NSScreen::screens(mtm).iter() {
        let f = screen.frame();
        if loc.x >= f.origin.x
            && loc.x < f.origin.x + f.size.width
            && loc.y >= f.origin.y
            && loc.y < f.origin.y + f.size.height
        {
            return Some(screen.visibleFrame());
        }
    }
    NSScreen::mainScreen(mtm).map(|s| s.visibleFrame())
}

/// 水平居中、垂直靠上（顶边距 ≈ visible 高 18%，Alfred / 启动器常见位置）。
const TOP_INSET_RATIO: f64 = 0.18;

fn placement_frame_on_vis(vis: NSRect, w: f64, h: f64) -> NSRect {
    use objc2_foundation::{NSPoint, NSSize};
    let w = w.min(vis.size.width).max(100.0);
    let h = h.min(vis.size.height).max(100.0);
    let x = vis.origin.x + (vis.size.width - w) / 2.0;
    let top_inset = vis.size.height * TOP_INSET_RATIO;
    // Cocoa：y 为底边；顶边 = vis 顶 - inset
    let mut y = vis.origin.y + vis.size.height - top_inset - h;
    if y < vis.origin.y {
        y = vis.origin.y;
    }
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}

fn apply_frame_no_anim(ns_window: &NSWindow, frame: NSRect) {
    unsafe {
        let cls = objc2::class!(NSAnimationContext);
        let _: () = objc2::msg_send![cls, beginGrouping];
        let ctx: *mut objc2::runtime::AnyObject = objc2::msg_send![cls, currentContext];
        let _: () = objc2::msg_send![ctx, setDuration: 0.0_f64];
        let _: () = objc2::msg_send![ns_window, setFrame: frame, display: true];
        let _: () = objc2::msg_send![cls, endGrouping];
    }
}

/// 主窗 show 专用：在光标屏居中并前置。写入 PLACEMENT_VIS，供 animate_frame 只改尺寸。
pub fn present_on_cursor_screen(window: &tauri::WebviewWindow) {
    use objc2_app_kit::NSWindowCollectionBehavior;

    let Ok(ptr) = window.ns_window() else {
        return;
    };
    let raw = ptr.cast::<NSWindow>();
    let Some(ns_window) = (unsafe { raw.as_ref() }) else {
        return;
    };
    let Some(vis) = cursor_visible_frame() else {
        return;
    };
    store_placement(vis);

    let cur = ns_window.frame();
    let w = if cur.size.width >= 100.0 {
        cur.size.width
    } else {
        MAIN_DEFAULT_W
    };
    let h = if cur.size.height >= 100.0 {
        cur.size.height
    } else {
        MAIN_DEFAULT_H
    };
    let frame = placement_frame_on_vis(vis, w, h);

    // 每次 present 重申：CanJoinAllSpaces（勿加 MoveToActiveSpace）
    let behavior = NSWindowCollectionBehavior::CanJoinAllSpaces
        | NSWindowCollectionBehavior::FullScreenAuxiliary;
    ns_window.setCollectionBehavior(behavior);
    ns_window.setLevel(objc2_app_kit::NSFloatingWindowLevel);
    ns_window.setHasShadow(true);

    // 先定位再露脸：hide 用 alpha=0 不 orderOut，跨屏 setFrame 更稳
    apply_frame_no_anim(ns_window, frame);
    ns_window.setHasShadow(true);
    ns_window.setAlphaValue(1.0);
    ns_window.orderFrontRegardless();
    apply_frame_no_anim(ns_window, frame);
    capture_mouse_events(ns_window);

    let window_number: objc2_foundation::NSInteger =
        unsafe { objc2::msg_send![ns_window, windowNumber] };
    crate::platform::skylight::set_full_event_shape(
        window_number as i64,
        frame.size.width,
        frame.size.height,
    );
}

/// 隐藏：resignKey + 去阴影 + alpha=0 + 忽略鼠标。**不 orderOut**。
///
/// 副屏二次 show 失败的主因：orderOut 后窗口脱离扩展屏 Space/显示链路，
/// 即使 setFrame 坐标正确也完全不绘。保持在窗口列表内，仅透明隐藏。
///
/// 注意：alpha=0 时 NSWindow.isVisible 仍可能为 true；业务可见性以
/// `shortcut::WINDOW_VISIBLE` 为准，勿仅依赖 Tauri is_visible。
pub fn hide_native(window: &tauri::WebviewWindow) {
    cancel_pending_present();
    if let Ok(raw) = window.ns_window() {
        let raw = raw.cast::<NSWindow>();
        if let Some(ns_window) = unsafe { raw.as_ref() } {
            ns_window.resignKeyWindow();
            ns_window.setHasShadow(false);
            ns_window.setIgnoresMouseEvents(true);
            ns_window.setAlphaValue(0.0);
            // 刻意不 orderOut
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

/// 主窗高度/尺寸动画。
///
/// 只在 `PLACEMENT_VIS`（show 时锁定的光标屏）内改尺寸，忽略前端 x/y 与
/// NSWindow.screen——跨屏 show 后前端滞后坐标不会把窗拽回主屏；高度可立即生效。
pub fn animate_frame(window: &tauri::WebviewWindow, _x: f64, _y: f64, w: f64, h: f64) {
    use objc2_foundation::{ns_string, NSPoint, NSRect, NSSize};
    const DURATION_SECS: f64 = 0.26;
    const BOTTOM_MARGIN: f64 = 40.0;

    let Ok(ptr) = window.ns_window() else {
        return;
    };
    let raw = ptr.cast::<NSWindow>();
    let Some(ns_window) = (unsafe { raw.as_ref() }) else {
        return;
    };

    let vis = load_placement()
        .map(|p| p.to_ns())
        .or_else(cursor_visible_frame);
    let Some(vis) = vis else {
        return;
    };

    let mut w = w.clamp(100.0, vis.size.width.max(100.0));
    let mut h = h.clamp(100.0, (vis.size.height * 0.9).max(100.0));
    if w > vis.size.width {
        w = vis.size.width;
    }
    if h > vis.size.height {
        h = vis.size.height;
    }

    let cur = ns_window.frame();
    // 与 present 同策略：水平居中、顶边优先靠上；高度变化时尽量保顶边
    let x = vis.origin.x + (vis.size.width - w) / 2.0;
    let top = cur.origin.y + cur.size.height;
    let mut y = top - h;
    let top_on_screen = top >= vis.origin.y - 1.0 && top <= vis.origin.y + vis.size.height + 1.0;
    let x_overlap = cur.origin.x + cur.size.width > vis.origin.x
        && cur.origin.x < vis.origin.x + vis.size.width;
    if !top_on_screen || !x_overlap || cur.size.width < 50.0 {
        // 不在 placement 屏：重新按靠上规则放置
        let placed = placement_frame_on_vis(vis, w, h);
        y = placed.origin.y;
    } else {
        // 底部将出屏则上移；顶边不超过 visible 顶
        if y < vis.origin.y + BOTTOM_MARGIN {
            y = vis.origin.y + BOTTOM_MARGIN;
        }
        let max_top = vis.origin.y + vis.size.height;
        if y + h > max_top {
            y = max_top - h;
        }
        if y < vis.origin.y {
            y = vis.origin.y;
        }
    }

    let frame = NSRect::new(NSPoint::new(x, y), NSSize::new(w, h));
    unsafe {
        let ctx_cls = objc2::class!(NSAnimationContext);
        let _: () = objc2::msg_send![ctx_cls, beginGrouping];
        let ctx: *mut objc2::runtime::AnyObject = objc2::msg_send![ctx_cls, currentContext];
        let _: () = objc2::msg_send![ctx, setDuration: DURATION_SECS];
        let timing_cls = objc2::class!(CAMediaTimingFunction);
        let timing: *mut objc2::runtime::AnyObject =
            objc2::msg_send![timing_cls, functionWithName: ns_string!("default")];
        let _: () = objc2::msg_send![ctx, setTimingFunction: timing];
        let animator: *mut objc2::runtime::AnyObject = objc2::msg_send![ns_window, animator];
        let _: () = objc2::msg_send![animator, setFrame: frame, display: true];
        let _: () = objc2::msg_send![ctx_cls, endGrouping];
    }
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

    ns_window.setOpaque(false);
    ns_window.setBackgroundColor(Some(&NSColor::clearColor()));

    let Some(content_view) = ns_window.contentView() else {
        return;
    };

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

    unsafe {
        let _: () = objc2::msg_send![&content_view, setWantsLayer: true];
        let layer: *mut objc2::runtime::AnyObject = objc2::msg_send![&content_view, layer];
        if !layer.is_null() {
            let _: () = objc2::msg_send![layer, setCornerRadius: corner_radius];
            let _: () = objc2::msg_send![layer, setMasksToBounds: true];
        }
    }

    unsafe {
        for sv in content_view.subviews().iter() {
            let _: () = objc2::msg_send![&*sv, setWantsLayer: true];
            let sv_layer: *mut objc2::runtime::AnyObject = objc2::msg_send![&*sv, layer];
            if !sv_layer.is_null() {
                let _: () = objc2::msg_send![sv_layer, setOpaque: false];
            }
        }
    }

    let mtm = MainThreadMarker::new().expect("on main thread");
    let effect =
        NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), content_view.bounds());
    effect.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    effect.setMaterial(NSVisualEffectMaterial::HeaderView);
    effect.setState(NSVisualEffectState::Active);
    if let Some(aqua) = NSAppearance::appearanceNamed(ns_string!("NSAppearanceNameAqua")) {
        effect.setAppearance(Some(&aqua));
    }
    effect.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    content_view.addSubview_positioned_relativeTo(&effect, NSWindowOrderingMode::Below, None);
}

/// 透明面板强制接管鼠标：禁止 ignore + 收 mouseMoved + 全窗 event shape（不依赖 alpha）。
pub fn capture_mouse_events(ns_window: &NSWindow) {
    ns_window.setIgnoresMouseEvents(false);
    ns_window.setAcceptsMouseMovedEvents(true);
    crate::platform::skylight::set_full_event_shape_for_nswindow(ns_window);
}

/// 主窗口框架级样式：Mica 材质底（apply_mica_material）+ NonactivatingPanel 转换。
pub fn apply_main_window_style(window: &tauri::WebviewWindow) {
    use objc2_app_kit::NSWindowCollectionBehavior;

    let Ok(ptr) = window.ns_window() else {
        return;
    };
    let raw = ptr.cast::<NSWindow>();
    let Some(ns_window) = (unsafe { raw.as_ref() }) else {
        return;
    };
    apply_mica_material(ns_window, 16.0);
    ns_window.setHasShadow(true);
    crate::platform::panel::convert_to_panel(raw.cast());
    // 多屏：CanJoinAllSpaces（可出现在各屏 Space）。
    // 勿与 MoveToActiveSpace 并用——二者互斥，组合会在 did_finish_launching 触发
    // 「panic in a function that cannot unwind」级崩溃。
    // 去掉 Transient（副屏二次 orderOut 后可能无法再 orderFront）。
    let behavior = NSWindowCollectionBehavior::CanJoinAllSpaces
        | NSWindowCollectionBehavior::FullScreenAuxiliary;
    ns_window.setCollectionBehavior(behavior);
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
