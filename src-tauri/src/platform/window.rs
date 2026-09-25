//! 主窗口 macOS 原生操作（NSWindow / NSOpenPanel）。platform 层纯原语，
//! runtime/window.rs 负责编排（可见性状态 / click_monitor / focus 还原）。
//!
//! NonactivatingPanel + LSUIElement 模式下,orderFrontRegardless + makeKeyWindow
//! 让面板取得键盘焦点,而不抢 NSApp active —— 原前台应用的菜单栏 / Dock 高亮
//! 全程不变,视觉上像浮层从未离开过当前应用。

use crate::runtime::lock_or_recover;
use objc2_app_kit::NSWindow;
use objc2_foundation::NSRect;
use std::sync::Mutex;

/// 本进程任一 NSMenu 菜单窗口是否在屏（托盘下拉 / 右键菜单打开期间为真）。
/// menubar 据此推迟重建——set_menu 替换 NSMenu 会立即关闭正在浏览的菜单。
/// 主线程无响应时按打开处理（保守：宁可推迟重建，不关用户的菜单）。
pub fn is_menu_open(app: &tauri::AppHandle) -> bool {
    let (tx, rx) = std::sync::mpsc::channel();
    let _ = app.run_on_main_thread(move || {
        let open = objc2_foundation::MainThreadMarker::new()
            .map(|mtm| {
                use objc2_app_kit::NSApp;
                NSApp(mtm).windows().iter().any(|w| {
                    w.class()
                        .name()
                        .to_str()
                        .is_ok_and(|n| n.contains("MenuWindow"))
                        && w.isVisible()
                })
            })
            .unwrap_or(false);
        let _ = tx.send(open);
    });
    rx.recv_timeout(std::time::Duration::from_millis(500))
        .unwrap_or(true)
}

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

/// 最近一次 set_window_appearance 锁定的 mode（"light"/"dark"/"auto"）。
/// None = 未设置（首次启动，等价跟随系统）。invisible 创建的子窗口（screenshot/
/// snap-panel）经 apply_cached_appearance 据此设原生 appearance；visible 创建的
/// pin 窗口不可设（setAppearance 死锁），改由前端 get_cached_appearance 命令读取。
static WINDOW_APPEARANCE: Mutex<Option<String>> = Mutex::new(None);

fn store_placement(vis: NSRect) {
    *lock_or_recover(&PLACEMENT_VIS) = Some(PlacementVis::from_ns(vis));
}

fn load_placement() -> Option<PlacementVis> {
    *lock_or_recover(&PLACEMENT_VIS)
}

/// hide 时调用：清掉 show 时锁定的 placement，避免隐藏态仍按旧屏改尺寸。
pub fn cancel_pending_present() {
    *lock_or_recover(&PLACEMENT_VIS) = None;
}

/// 主窗口高度天花板：placement 屏（show 时锁定）优先、否则光标屏的 visibleFrame × 0.9。
/// animate_frame 的 set_main_frame clamp 与前端 get_window_max_height 命令同源共用——
/// 前端若按整屏高（monitor 尺寸，含菜单栏/Dock）推导内容上限，会撑过 clamp 产生窗口级滚动。
pub fn main_window_height_ceiling() -> Option<f64> {
    let vis = load_placement()
        .map(|p| p.to_ns())
        .or_else(cursor_visible_frame)?;
    Some(height_ceiling(vis))
}

/// 主窗当前 frame（Cocoa 逻辑）+ placement/光标屏 visibleFrame。前端高度管理
/// （useExtensionHeight）的位置单一真相源：全 Cocoa 语义、零转换——Tauri 的
/// outerPosition 经 tao 转成 top-left（y 向下、参照主屏物理高）语义，与 NSWindow
/// setFrame（Cocoa、y 向上底边）错位，不可混用。
pub fn main_frame_and_vis(window: &tauri::WebviewWindow) -> Option<(NSRect, NSRect)> {
    let ptr = window.ns_window().ok()?;
    let raw = ptr.cast::<NSWindow>();
    let ns_window = (unsafe { raw.as_ref() })?;
    let frame = ns_window.frame();
    let vis = load_placement()
        .map(|p| p.to_ns())
        .or_else(cursor_visible_frame)?;
    Some((frame, vis))
}

/// 包含指定点（Cocoa）的屏 visibleFrame。
pub fn screen_vis_containing(
    mtm: objc2_foundation::MainThreadMarker,
    x: f64,
    y: f64,
) -> Option<NSRect> {
    use objc2_app_kit::NSScreen;
    NSScreen::screens(mtm)
        .iter()
        .map(|s| s.visibleFrame())
        .find(|v| {
            x >= v.origin.x
                && x < v.origin.x + v.size.width
                && y >= v.origin.y
                && y < v.origin.y + v.size.height
        })
}

/// 天花板公式（单一源）：visibleFrame × 0.9，下限 100。
pub fn height_ceiling(vis: NSRect) -> f64 {
    (vis.size.height * 0.9).max(100.0)
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

    // 先定位 + 恢复事件捕获，最后才露脸：hide 时 setIgnoresMouseEvents(true)，
    // 须在 setAlphaValue(1.0) 之前恢复 false——窗口服务器对视觉合成（alpha）与
    // hit-test 表（ignoresMouseEvents）的更新是异步步趋，若可见先于 hit-test 落地，
    // 滚动等事件会穿透到下层应用窗口（菜单栏关闭后窗口服务器过渡态下偶发）。
    apply_frame_no_anim(ns_window, frame);
    capture_mouse_events(ns_window);
    ns_window.setAlphaValue(1.0);
    ns_window.orderFrontRegardless();
    apply_frame_no_anim(ns_window, frame);
    // orderFront 后 frame 确认，重设 event shape 确保窗口服务器 hit-test 同步
    crate::platform::skylight::set_full_event_shape_for_nswindow(ns_window);
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
/// 位置（x/y）以前端 useExtensionHeight 为单一真相源（顶边锚定：fixed/default 保顶边、
/// auto 增高出屏上移、离开 auto 还原进入前顶边），本函数直接采用传入坐标并 clamp 进
/// `PLACEMENT_VIS`（show 时锁定的光标屏，扣菜单栏/Dock 的 visibleFrame）。
/// 跨屏异常（cur 不在 placement 屏）时仍复位居中。每次 show 经
/// present_on_cursor_screen 重定位，故拖动仅影响当前显示期间，下次唤起自动复位。
pub fn animate_frame(window: &tauri::WebviewWindow, x: f64, y: f64, w: f64, h: f64) {
    use objc2_foundation::{NSPoint, NSRect, NSSize};
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
    let mut h = h.clamp(100.0, height_ceiling(vis));
    if w > vis.size.width {
        w = vis.size.width;
    }
    if h > vis.size.height {
        h = vis.size.height;
    }

    let cur = ns_window.frame();
    let top = cur.origin.y + cur.size.height;
    let top_on_screen = top >= vis.origin.y - 1.0 && top <= vis.origin.y + vis.size.height + 1.0;
    let x_overlap = cur.origin.x + cur.size.width > vis.origin.x
        && cur.origin.x < vis.origin.x + vis.size.width;
    let (x, y) = if !top_on_screen || !x_overlap || cur.size.width < 50.0 {
        // 不在 placement 屏：重新居中放置（跨屏异常复位）
        let placed = placement_frame_on_vis(vis, w, h);
        (placed.origin.x, placed.origin.y)
    } else {
        // 在屏内：采用前端目标坐标（屏几何整屏近似，底部/顶边越界由 visibleFrame clamp 拉正）。
        // clamp 顺序沿旧策略：底部 40px 间距优先于顶边
        let x = x.clamp(
            vis.origin.x,
            (vis.origin.x + vis.size.width - w).max(vis.origin.x),
        );
        let mut y = y;
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
        // 顶边 clamp 可能把底边压进 margin 内（屏太矮装不下），以底部间距为最终约束
        if y < vis.origin.y + BOTTOM_MARGIN {
            y = vis.origin.y + BOTTOM_MARGIN;
        }
        (x, y)
    };

    let frame = NSRect::new(NSPoint::new(x, y), NSSize::new(w, h));
    unsafe { animator_set_frame(ns_window, frame) };
    let window_number: objc2_foundation::NSInteger =
        unsafe { objc2::msg_send![ns_window, windowNumber] };
    crate::platform::skylight::set_full_event_shape(window_number as i64, w, h);
}

/// NSAnimationContext 接管 setFrame（CoreAnimation 动画，系统级而非 JS rAF 逐帧）。
unsafe fn animator_set_frame(ns_window: &NSWindow, frame: NSRect) {
    use objc2_foundation::ns_string;
    const DURATION_SECS: f64 = 0.26;
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

/// 授权会话避让专用：主窗动画移到指定 frame（Cocoa），并把 placement 切到该屏——
/// 会话期间前端的高度/位置约束与目标屏一致；避让只影响当前显示期间，下次 show 仍按
/// 光标屏复位（与拖动同语义）。与 animate_frame 的差异：采用显式坐标不做跨屏复位
/// 居中（跨屏正是本函数的目的），clamp 进包含 frame 的屏 visibleFrame（底部 40 间距
/// 与 animate_frame 同规）。仅主线程调用。
pub fn move_main_to(window: &tauri::WebviewWindow, frame: NSRect) {
    use objc2_foundation::{MainThreadMarker, NSPoint, NSSize};

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Ok(ptr) = window.ns_window() else { return };
    let raw = ptr.cast::<NSWindow>();
    let Some(ns_window) = (unsafe { raw.as_ref() }) else {
        return;
    };

    let cx = frame.origin.x + frame.size.width / 2.0;
    let cy = frame.origin.y + frame.size.height / 2.0;
    let Some(vis) = screen_vis_containing(mtm, cx, cy) else {
        return;
    };
    store_placement(vis);

    const BOTTOM_MARGIN: f64 = 40.0;
    let w = frame.size.width.min(vis.size.width).max(100.0);
    let h = frame.size.height.min(height_ceiling(vis)).max(100.0);
    let x = frame.origin.x.clamp(
        vis.origin.x,
        (vis.origin.x + vis.size.width - w).max(vis.origin.x),
    );
    let mut y = frame.origin.y;
    if y < vis.origin.y + BOTTOM_MARGIN {
        y = vis.origin.y + BOTTOM_MARGIN;
    }
    if y + h > vis.origin.y + vis.size.height {
        y = vis.origin.y + vis.size.height - h;
    }
    if y < vis.origin.y + BOTTOM_MARGIN {
        y = vis.origin.y + BOTTOM_MARGIN;
    }

    let frame = NSRect::new(NSPoint::new(x, y), NSSize::new(w, h));
    unsafe { animator_set_frame(ns_window, frame) };
    let window_number: objc2_foundation::NSInteger =
        unsafe { objc2::msg_send![ns_window, windowNumber] };
    crate::platform::skylight::set_full_event_shape(window_number as i64, w, h);
}

/// 授权会话层级降级：主窗降到普通层级并置顶于该层。主窗常态浮动层级（present
/// 每次重申 NSFloatingWindowLevel），普通层级的系统设置窗口即使激活也盖不过浮动
/// 窗。降层置顶后由调用方紧接激活系统设置——设置窗被压到主窗之上，其余应用窗口
/// 在主窗之下（三明治）。orderWindow:relativeTo: 跨 app 排序实测无效，勿再用。
/// 会话结束经 restore_window_order 复位。仅主线程调用。
pub fn demote_window_level(window: &tauri::WebviewWindow) {
    let Ok(ptr) = window.ns_window() else { return };
    let raw = ptr.cast::<NSWindow>();
    let Some(ns_window) = (unsafe { raw.as_ref() }) else {
        return;
    };
    ns_window.setLevel(objc2_app_kit::NSNormalWindowLevel);
    ns_window.orderFrontRegardless();
}

/// 授权会话结束：复位浮动层级并前置；make_key 时 panel makeKey 取得键盘焦点
/// （不激活 NSApp——语义同 show 主链路）。完全访问/录屏授权后系统弹重启确认，
/// 夺 key 会把弹窗降为非激活，调用方须传 false 只置顶。
pub fn restore_window_order(window: &tauri::WebviewWindow, make_key: bool) {
    let Ok(ptr) = window.ns_window() else { return };
    let raw = ptr.cast::<NSWindow>();
    let Some(ns_window) = (unsafe { raw.as_ref() }) else {
        return;
    };
    ns_window.setLevel(objc2_app_kit::NSFloatingWindowLevel);
    ns_window.orderFrontRegardless();
    if make_key {
        ns_window.makeKeyWindow();
    }
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
/// 再叠前端 `mica-tint` 染色，得到磨砂玻璃而非「纯模糊透壁纸」。
/// appearance 跟随主题（auto 跟随系统 / light·dark 强制），见 apply_window_appearance。
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
        NSAutoresizingMaskOptions, NSColor, NSVisualEffectBlendingMode, NSVisualEffectMaterial,
        NSVisualEffectState, NSVisualEffectView, NSWindowOrderingMode,
    };

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
    // appearance 不在此锁：由 apply_window_appearance 统一设置 NSWindow appearance，
    // effect view 与 WKWebView 均继承之（auto 跟随系统 / light·dark 强制）。
    effect.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    content_view.addSubview_positioned_relativeTo(&effect, NSWindowOrderingMode::Below, None);
}

/// 设置窗口外观（appearance）：`light` / `dark` 强制，`auto` / 其它 = None（跟随系统）。
///
/// NSWindow.setAppearance 同时作用于 NSVisualEffectView 材质与 WKWebView：
/// 后者的 `prefers-color-scheme` media query 随之改变。因此 auto 模式必须传 None，
/// 使其反映系统真实外观——前端 theme.ts 据此 matchMedia 决定 DOM data-theme；
/// light/dark 则强制覆盖（此时前端不再读 matchMedia，直接按选择值设 DOM）。
pub fn apply_window_appearance(window: &tauri::WebviewWindow, mode: &str) {
    use objc2_app_kit::{NSAppearance, NSAppearanceCustomization};
    use objc2_foundation::ns_string;

    // 缓存 mode：invisible 子窗口（screenshot/snap-panel）经 apply_cached_appearance
    // 据此设原生 appearance；pin 窗口经 get_cached_appearance 命令由前端读取。
    *lock_or_recover(&WINDOW_APPEARANCE) = Some(mode.to_string());

    let Ok(ptr) = window.ns_window() else {
        return;
    };
    let raw = ptr.cast::<NSWindow>();
    let Some(ns_window) = (unsafe { raw.as_ref() }) else {
        return;
    };
    let appearance = match mode {
        "light" => NSAppearance::appearanceNamed(ns_string!("NSAppearanceNameAqua")),
        "dark" => NSAppearance::appearanceNamed(ns_string!("NSAppearanceNameDarkAqua")),
        // auto / 未知：None = 跟随系统
        _ => None,
    };
    ns_window.setAppearance(appearance.as_deref());
}

/// invisible 创建的子窗口（screenshot/snap-panel）调用：按缓存的 mode 设原生 appearance。
/// pin 窗口 visible 创建不调此函数（setAppearance 死锁），改由前端 get_cached_appearance 读取。
pub fn apply_cached_appearance(window: &tauri::WebviewWindow) {
    // mode 独立语句绑定：guard 在分号处 drop 释放锁，避免 if let scrutinee 临时值
    // 生命周期延续到块结束、块内 apply_window_appearance 重入同一把锁自死锁。
    let mode = lock_or_recover(&WINDOW_APPEARANCE).clone();
    if let Some(mode) = mode {
        apply_window_appearance(window, &mode);
    }
}

/// 返回缓存的 appearance mode（auto/light/dark）。pin 窗口前端读取后直接设 DOM data-theme，
/// 替代不可用的 matchMedia（pin 未设 setAppearance，prefers-color-scheme 跟随系统）。
pub fn cached_appearance() -> Option<String> {
    lock_or_recover(&WINDOW_APPEARANCE).clone()
}

/// 透明面板强制接管鼠标：禁止 ignore + 收 mouseMoved + 全窗 event shape（不依赖 alpha）。
pub fn capture_mouse_events(ns_window: &NSWindow) {
    ns_window.setIgnoresMouseEvents(false);
    ns_window.setAcceptsMouseMovedEvents(true);
    crate::platform::skylight::set_full_event_shape_for_nswindow(ns_window);
}

/// 延迟刷新事件捕获（仅窗口仍可见时执行）。
/// 菜单栏菜单关闭后窗口服务器 hit-test 表可能滞后更新，导致滚动事件穿透；
/// show 后延迟重应用 capture_mouse_events 强制刷新 hit-test。
pub fn refresh_event_capture_if_visible(window: &tauri::WebviewWindow) {
    let Ok(ptr) = window.ns_window() else {
        return;
    };
    let raw = ptr.cast::<NSWindow>();
    let Some(ns_window) = (unsafe { raw.as_ref() }) else {
        return;
    };
    // 仅在窗口仍可见时执行（hide 后 alpha=0 跳过，避免对已隐藏窗口无效操作）
    if ns_window.alphaValue() < 0.01 {
        return;
    }
    capture_mouse_events(ns_window);
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
