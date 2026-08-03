use crate::runtime::lock_or_recover;
use std::sync::atomic::Ordering;
use std::sync::Mutex;

use super::ffi::{
    get_cg_image, picker_jpeg_path, store_cg_image, voidnix_screenshot_claim_key,
    voidnix_screenshot_clear_background, voidnix_screenshot_get_mouse_location,
    voidnix_screenshot_prewarm, voidnix_screenshot_set_background, ScreenshotData, WindowRect,
};

/// 当前截屏会话的目标显示器（Quartz 全局原点 + 逻辑尺寸）。
/// 前端选区始终为屏内本地坐标；pin / scroll 等出口在 native 侧加 origin。
#[derive(Clone, Copy, Debug)]
pub struct CaptureSurface {
    pub display_id: u32,
    pub origin_x: f64,
    pub origin_y: f64,
    pub width: f64,
    pub height: f64,
}

static CAPTURE_SURFACE: Mutex<Option<CaptureSurface>> = Mutex::new(None);

/// 当前会话目标屏 Quartz 原点（无会话时 (0,0)，兼容主屏）。
pub fn capture_origin() -> (f64, f64) {
    CAPTURE_SURFACE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .map(|s| (s.origin_x, s.origin_y))
        .unwrap_or((0.0, 0.0))
}

pub fn capture_surface() -> Option<CaptureSurface> {
    *lock_or_recover(&CAPTURE_SURFACE)
}

fn store_capture_surface(surface: CaptureSurface) {
    *lock_or_recover(&CAPTURE_SURFACE) = Some(surface);
}

fn clear_capture_surface() {
    *lock_or_recover(&CAPTURE_SURFACE) = None;
}

/// capture 成功但 enter 调度失败时的清理：flag / surface / CGImage / 在途 picker。
#[cfg(target_os = "macos")]
pub(super) fn cleanup_failed_capture() {
    IS_IN_SCREENSHOT_SESSION.store(false, Ordering::SeqCst);
    clear_capture_surface();
    super::ffi::cancel_picker_jobs();
    super::ffi::store_cg_image(std::ptr::null_mut());
}

/// 全局 Quartz 鼠标 → 目标屏本地坐标（左上原点）。
fn global_to_local(gx: f64, gy: f64, surface: &CaptureSurface) -> (f64, f64) {
    let lx = (gx - surface.origin_x).clamp(0.0, surface.width);
    let ly = (gy - surface.origin_y).clamp(0.0, surface.height);
    (lx, ly)
}

#[cfg(target_os = "macos")]
fn quartz_mouse_location() -> (f64, f64) {
    let mut x: f64 = 0.0;
    let mut y: f64 = 0.0;
    // SAFETY: 栈上 out 指针合法；FFI 薄壳写 f64
    unsafe {
        voidnix_screenshot_get_mouse_location(&mut x, &mut y, 0.0);
    }
    (x, y)
}

/// 光标所在 CGDisplay（Quartz 点命中；失败回落主屏）。可在任意线程调用。
///
/// 不用 core-graphics 的 `displays_with_point`：其把 `max_displays` 传给
/// `CGGetDisplaysWithPoint` 但 buffer 只按 count 分配，镜像/重叠屏时可能越界。
/// 且须先 `CGMainDisplayID` 完成 CGS 初始化，否则后台线程偶发 assert/失败。
#[cfg(target_os = "macos")]
fn resolve_display_under_cursor() -> core_graphics::display::CGDisplay {
    use core_graphics::base::kCGErrorSuccess;
    use core_graphics::display::{
        CGDirectDisplayID, CGDisplay, CGGetDisplaysWithPoint, CGMainDisplayID,
    };
    use core_graphics::geometry::CGPoint;

    // CGS 初始化：见 core-graphics 测试注释（CGGetDisplays* 前须先 main）
    let _ = unsafe { CGMainDisplayID() };

    let (mx, my) = quartz_mouse_location();
    let point = CGPoint { x: mx, y: my };
    const MAX: u32 = 16;
    let mut ids: [CGDirectDisplayID; MAX as usize] = [0; MAX as usize];
    let mut matching: u32 = 0;
    // SAFETY: ids 容量 = MAX；CGGetDisplaysWithPoint 最多写 MAX 个 id
    let err = unsafe { CGGetDisplaysWithPoint(point, MAX, ids.as_mut_ptr(), &mut matching) };
    if err == kCGErrorSuccess && matching > 0 {
        CGDisplay::new(ids[0])
    } else {
        CGDisplay::main()
    }
}

/// 按 CGDirectDisplayID 找 NSScreen.frame（主线程）。
#[cfg(target_os = "macos")]
fn ns_screen_frame_for_display(
    display_id: u32,
    mtm: objc2_foundation::MainThreadMarker,
) -> Option<objc2_foundation::NSRect> {
    use objc2_app_kit::NSScreen;
    use objc2_foundation::NSString;

    let screens = NSScreen::screens(mtm);
    let key = NSString::from_str("NSScreenNumber");
    for screen in screens.iter() {
        let desc = screen.deviceDescription();
        let Some(obj) = desc.objectForKey(&key) else {
            continue;
        };
        // SAFETY: deviceDescription[@"NSScreenNumber"] 为 NSNumber
        let sid: u32 = unsafe { objc2::msg_send![&*obj, unsignedIntValue] };
        if sid == display_id {
            return Some(screen.frame());
        }
    }
    None
}

/// Cocoa 鼠标所在 NSScreen.frame（主线程；不依赖 NSScreenNumber ↔ CGDisplay 对齐）。
#[cfg(target_os = "macos")]
fn ns_screen_frame_under_cursor(
    mtm: objc2_foundation::MainThreadMarker,
) -> Option<objc2_foundation::NSRect> {
    use objc2_app_kit::{NSEvent, NSScreen};

    let loc = NSEvent::mouseLocation();
    for screen in NSScreen::screens(mtm).iter() {
        let f = screen.frame();
        if loc.x >= f.origin.x
            && loc.x < f.origin.x + f.size.width
            && loc.y >= f.origin.y
            && loc.y < f.origin.y + f.size.height
        {
            return Some(f);
        }
    }
    None
}

/// 抓取指定显示器画面：优先 CGDisplayCreateImage，失败用 CGWindowListCreateImage
///（外接/HDR 屏上 CreateImage 偶发 null，WindowList 更稳；不回落主屏以免副屏「没出来」）。
#[cfg(target_os = "macos")]
fn capture_display_image(
    display: core_graphics::display::CGDisplay,
) -> Option<core_graphics::image::CGImage> {
    use core_graphics::display::CGDisplay;
    use core_graphics::window::{
        kCGNullWindowID, kCGWindowImageBestResolution, kCGWindowImageDefault,
        kCGWindowListOptionOnScreenOnly,
    };

    if let Some(img) = display.image() {
        return Some(img);
    }
    eprintln!(
        "[shot] CGDisplayCreateImage(id={}) 失败，改用 CGWindowListCreateImage",
        display.id
    );
    CGDisplay::screenshot(
        display.bounds(),
        kCGWindowListOptionOnScreenOnly,
        kCGNullWindowID,
        kCGWindowImageDefault | kCGWindowImageBestResolution,
    )
}

#[cfg(target_os = "macos")]
type CompletionCallback = Box<dyn FnOnce() + Send + 'static>;

#[cfg(target_os = "macos")]
pub(super) fn fade_window_layer_opacity(
    ns_window_addr: usize,
    target: f32,
    duration: f64,
    completion: Option<CompletionCallback>,
) {
    use objc2::runtime::AnyObject;
    use objc2_foundation::NSString;
    use std::sync::{Arc, Mutex};

    // SAFETY: nsw = ns_window_addr 转 AnyObject 裸指针（调用方传入合法 NSWindow 地址）；
    // 所有 msg_send 为 NSView/CALayer/CABasicAnimation/NSNumber/CATransaction 标准选择子，
    // 参数类型匹配；contentView/layer/anim 返回值均 null 检查；completion 回调经
    // RcBlock 持有，CATransaction setCompletionBlock: retain 后随 commit 触发
    unsafe {
        let nsw = ns_window_addr as *mut AnyObject;
        let content_view: *mut AnyObject = objc2::msg_send![nsw, contentView];
        if content_view.is_null() {
            if let Some(cb) = completion {
                cb();
            }
            return;
        }
        let _: () = objc2::msg_send![content_view, setWantsLayer: true];
        let layer: *mut AnyObject = objc2::msg_send![content_view, layer];
        if layer.is_null() {
            if let Some(cb) = completion {
                cb();
            }
            return;
        }

        let from_opacity: f32 = objc2::msg_send![layer, opacity];
        let cls_anim = objc2::class!(CABasicAnimation);
        let key_path = NSString::from_str("opacity");
        let anim: *mut AnyObject = objc2::msg_send![cls_anim, animationWithKeyPath: &*key_path];

        let cls_num = objc2::class!(NSNumber);
        let from_val: *mut AnyObject = objc2::msg_send![cls_num, numberWithFloat: from_opacity];
        let to_val: *mut AnyObject = objc2::msg_send![cls_num, numberWithFloat: target];
        let _: () = objc2::msg_send![anim, setFromValue: from_val];
        let _: () = objc2::msg_send![anim, setToValue: to_val];
        let _: () = objc2::msg_send![anim, setDuration: duration];

        let _: () = objc2::msg_send![layer, setOpacity: target];

        let anim_key = NSString::from_str("voidnix-fade-opacity");
        let cls_ct = objc2::class!(CATransaction);

        match completion {
            Some(cb) => {
                let slot: Arc<Mutex<Option<CompletionCallback>>> = Arc::new(Mutex::new(Some(cb)));
                let slot_clone = Arc::clone(&slot);
                let done = block2::RcBlock::new(move || {
                    if let Some(f) = lock_or_recover(&slot_clone).take() {
                        f();
                    }
                });
                let _: () = objc2::msg_send![cls_ct, begin];
                let _: () = objc2::msg_send![cls_ct, setCompletionBlock: &*done];
                let _: () = objc2::msg_send![layer, addAnimation: anim, forKey: &*anim_key];
                let _: () = objc2::msg_send![cls_ct, commit];
            }
            None => {
                let _: () = objc2::msg_send![layer, addAnimation: anim, forKey: &*anim_key];
            }
        }
    }
}

#[cfg(target_os = "macos")]
pub(super) fn set_window_layer_opacity(ns_window_addr: usize, opacity: f32) {
    use objc2::runtime::AnyObject;
    // SAFETY: nsw = ns_window_addr 转 AnyObject 裸指针（调用方传入合法 NSWindow 地址）；
    // contentView/layer null 检查；CATransaction 禁用隐式动画后 setOpacity: 立即生效
    unsafe {
        let nsw = ns_window_addr as *mut AnyObject;
        let content_view: *mut AnyObject = objc2::msg_send![nsw, contentView];
        if content_view.is_null() {
            return;
        }
        let _: () = objc2::msg_send![content_view, setWantsLayer: true];
        let layer: *mut AnyObject = objc2::msg_send![content_view, layer];
        if layer.is_null() {
            return;
        }
        let cls_ct = objc2::class!(CATransaction);
        let _: () = objc2::msg_send![cls_ct, begin];
        let _: () = objc2::msg_send![cls_ct, setDisableActions: true];
        let _: () = objc2::msg_send![layer, setOpacity: opacity];
        let _: () = objc2::msg_send![cls_ct, commit];
    }
}

#[cfg(target_os = "macos")]
mod mouse_tracker {
    use crate::runtime::lock_or_recover;
    use objc2::runtime::AnyObject;
    use std::sync::Mutex;
    use tauri::Manager;

    struct SendObj(*mut AnyObject);
    unsafe impl Send for SendObj {}
    unsafe impl Sync for SendObj {}

    static GLOBAL_MONITOR: Mutex<SendObj> = Mutex::new(SendObj(std::ptr::null_mut()));
    static LOCAL_MONITOR: Mutex<SendObj> = Mutex::new(SendObj(std::ptr::null_mut()));

    // NSEventType: LeftMouseDown=1 Up=2 Moved=5 LeftDragged=6；flagsChanged 等不需
    // 冷启动后 WebView 常吞首击（系统当激活）；由 local monitor 转 JS 驱动选区。
    const MASK: u64 = (1u64 << 1) | (1u64 << 2) | (1u64 << 5) | (1u64 << 6);

    pub fn start(app: &tauri::AppHandle) {
        use objc2::ClassType;
        use objc2_app_kit::NSEvent;
        {
            let g = lock_or_recover(&GLOBAL_MONITOR);
            if !g.0.is_null() {
                return;
            }
        }
        {
            let app = app.clone();
            // 全局监视：冷启动时窗口未真正 key，首击会落到下层 app；
            // local 收不到，靠 global 在命中我们 surface 时注入 JS 并抢 key。
            let blk = block2::RcBlock::new(move |event: *mut AnyObject| {
                // SAFETY: event 为 AppKit 传入的 NSEvent*
                unsafe {
                    dispatch_pointer(&app, event, true);
                }
            });
            // SAFETY: mask 为标准 NSEvent 位掩码；block 经 RcBlock 持有
            unsafe {
                let m: *mut AnyObject = objc2::msg_send![NSEvent::class(), addGlobalMonitorForEventsMatchingMask: MASK, handler: &*blk];
                if !m.is_null() {
                    let _: () = objc2::msg_send![m, retain];
                    *lock_or_recover(&GLOBAL_MONITOR) = SendObj(m);
                    std::mem::forget(blk);
                }
            }
        }
        {
            let app = app.clone();
            let blk = block2::RcBlock::new(move |event: *mut AnyObject| -> *mut AnyObject {
                // SAFETY: event 为 AppKit 传入的 NSEvent*
                unsafe {
                    dispatch_pointer(&app, event, false);
                }
                // 不吞事件：标注工具栏 / scroll 穿透依赖 DOM；前端对 native 注入去重
                event
            });
            // SAFETY: local monitor 透传 event
            unsafe {
                let m: *mut AnyObject = objc2::msg_send![NSEvent::class(), addLocalMonitorForEventsMatchingMask: MASK, handler: &*blk];
                if !m.is_null() {
                    let _: () = objc2::msg_send![m, retain];
                    *lock_or_recover(&LOCAL_MONITOR) = SendObj(m);
                    std::mem::forget(blk);
                }
            }
        }
    }

    pub fn stop() {
        use objc2::ClassType;
        use objc2_app_kit::NSEvent;
        for slot in [&GLOBAL_MONITOR, &LOCAL_MONITOR] {
            let mut g = lock_or_recover(slot);
            if !g.0.is_null() {
                // SAFETY: g.0 非 null；removeMonitor + release 与 start 配对
                unsafe {
                    let _: () = objc2::msg_send![NSEvent::class(), removeMonitor: g.0];
                    let _: () = objc2::msg_send![g.0, release];
                }
                *g = SendObj(std::ptr::null_mut());
            }
        }
    }

    fn ensure_key(app: &tauri::AppHandle) {
        // SAFETY: ns 非空校验；会话中丢 key 时重 claim
        if let Some(raw) = crate::extensions::screenshot::screenshot_ns_window(app) {
            unsafe {
                if let Some(ns) = raw.as_ref() {
                    let on_space: bool = objc2::msg_send![ns, isOnActiveSpace];
                    if ns.alphaValue() > 0.5 && on_space && !ns.isKeyWindow() {
                        let ptr = raw as *mut std::ffi::c_void;
                        super::super::ffi::voidnix_screenshot_claim_key(ptr);
                    }
                }
            }
        }
    }

    /// 指针 → JS。
    /// `from_global`：全局监视只在光标落在当前 capture surface 内时注入
    ///（否则会误吃其他屏幕上的点击）。
    ///
    /// SAFETY: event 必须为有效 NSEvent*。
    unsafe fn dispatch_pointer(app: &tauri::AppHandle, event: *mut AnyObject, from_global: bool) {
        if event.is_null() {
            return;
        }
        let Some(window) = app.get_webview_window("screenshot") else {
            return;
        };
        let Some(surface) = super::capture_surface() else {
            return;
        };

        let (gx, gy) = super::quartz_mouse_location();
        // 全局路径：点击若因未 key 穿到下层，仍落在我们冻住的屏矩形内则接管
        if from_global {
            let in_surface = gx >= surface.origin_x
                && gx < surface.origin_x + surface.width
                && gy >= surface.origin_y
                && gy < surface.origin_y + surface.height;
            if !in_surface {
                return;
            }
            // 已是 key 时 local monitor 会覆盖同一事件，跳过避免双份 eval
            if let Ok(raw) = window
                .ns_window()
                .map(|p| p.cast::<objc2_app_kit::NSWindow>())
            {
                // SAFETY: ns 非空校验
                if let Some(ns) = unsafe { raw.as_ref() } {
                    if ns.isKeyWindow() {
                        return;
                    }
                }
            }
        }

        ensure_key(app);

        // NSEvent.type：1 down / 2 up / 5 moved / 6 dragged
        let etype: u64 = objc2::msg_send![event, type];
        let flags: u64 = objc2::msg_send![event, modifierFlags];
        let shift = (flags & (1u64 << 17)) != 0; // NSEventModifierFlagShift

        let (cx, cy) = super::global_to_local(gx, gy, &surface);

        let _ = window.eval(format!(
            "window.__setScreenshotCross && window.__setScreenshotCross({},{})",
            cx, cy
        ));

        let kind = match etype {
            1 => "down",
            2 => "up",
            5 | 6 => "move",
            _ => return,
        };

        let _ = window.eval(format!(
            "window.__screenshotPointer && window.__screenshotPointer('{kind}',{cx},{cy},{shift})"
        ));
    }
}

#[cfg(target_os = "macos")]
fn start_mouse_tracker(app: &tauri::AppHandle) {
    mouse_tracker::start(app);
}
#[cfg(target_os = "macos")]
fn stop_mouse_tracker() {
    mouse_tracker::stop();
}
#[cfg(not(target_os = "macos"))]
fn start_mouse_tracker(_app: &tauri::AppHandle) {}
#[cfg(not(target_os = "macos"))]
fn stop_mouse_tracker() {}

#[cfg(target_os = "macos")]
fn enumerate_visible_windows(
    origin_x: f64,
    origin_y: f64,
    screen_w: f64,
    screen_h: f64,
) -> Vec<WindowRect> {
    use crate::platform::window_list::{copy_on_screen_windows, dict_lookup};
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use std::ffi::c_void;

    let Some(array) = copy_on_screen_windows() else {
        return Vec::new();
    };

    let mut result = Vec::with_capacity(array.len() as usize);

    let key_layer = CFString::from_static_string("kCGWindowLayer");
    let key_bounds = CFString::from_static_string("kCGWindowBounds");
    let key_name = CFString::from_static_string("kCGWindowOwnerName");
    let key_alpha = CFString::from_static_string("kCGWindowAlpha");

    for i in 0..array.len() {
        let Some(dict) = array.get(i) else {
            continue;
        };

        let layer = dict_lookup(&dict, &key_layer)
            .and_then(|v| v.downcast::<CFNumber>())
            .and_then(|n| n.to_i64())
            .unwrap_or(-1);
        // 仅纳入普通内容窗口层级 [0, kCGMainMenuWindowLevel)：
        // 覆盖 Normal(0)/Floating(3, 含 Voidnix 主窗)/ModalPanel(8)/Utility(19)，
        // 排除菜单栏(24)/Status(25, 含截屏 overlay·钉图)/PopUpMenu 等 chrome。
        // 不再按 pid 排除本进程：否则主窗无法智能吸附；overlay 已由 layer≥24 滤掉。
        // Dock/桌面由 ExcludeDesktopElements 处理。
        const MAIN_MENU_LEVEL: i64 = 24;
        if !(0..MAIN_MENU_LEVEL).contains(&layer) {
            continue;
        }

        let alpha = dict_lookup(&dict, &key_alpha)
            .and_then(|v| v.downcast::<CFNumber>())
            .and_then(|n| n.to_f64())
            .unwrap_or(1.0);
        if alpha < 0.05 {
            continue;
        }

        let owner = dict_lookup(&dict, &key_name)
            .and_then(|v| v.downcast::<CFString>())
            .map(|s| s.to_string())
            .unwrap_or_default();

        let Some(bd) = dict_lookup(&dict, &key_bounds)
            .and_then(|v| v.downcast::<CFDictionary<*const c_void, *const c_void>>())
        else {
            continue;
        };

        let gn = |k: &'static str| -> f64 {
            let ks = CFString::from_static_string(k);
            dict_lookup(&bd, &ks)
                .and_then(|v| v.downcast::<CFNumber>())
                .and_then(|n| n.to_f64())
                .unwrap_or(0.0)
        };
        let (x, y, w, h) = (gn("X"), gn("Y"), gn("Width"), gn("Height"));
        if w < 40.0 || h < 40.0 {
            continue;
        }
        // 与目标屏相交（Quartz 全局），输出减 origin 后的本地坐标
        let sx2 = origin_x + screen_w;
        let sy2 = origin_y + screen_h;
        if x + w <= origin_x || x >= sx2 || y + h <= origin_y || y >= sy2 {
            continue;
        }
        result.push(WindowRect {
            x: x - origin_x,
            y: y - origin_y,
            w,
            h,
            owner,
        });
    }
    result
}

#[cfg(not(target_os = "macos"))]
fn enumerate_visible_windows(
    _origin_x: f64,
    _origin_y: f64,
    _screen_w: f64,
    _screen_h: f64,
) -> Vec<WindowRect> {
    Vec::new()
}

#[cfg(target_os = "macos")]
static SCREENSHOT_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(target_os = "macos")]
pub(super) static IS_IN_SCREENSHOT_SESSION: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// 捕获光标所在屏（仅 Rust 内部：快捷键路径 → enter_screenshot_mode_sync；不暴露 IPC）。
/// 可在后台线程调用（CGDisplay / CGEvent 线程安全；NSScreen 仅 enter 主线程使用）。
pub fn capture_screen() -> Result<ScreenshotData, String> {
    #[cfg(target_os = "macos")]
    {
        let display = resolve_display_under_cursor();
        let cg_image = capture_display_image(display)
            .ok_or("截屏失败：请在「系统设置 → 隐私与安全性 → 屏幕录制」中授权")?;

        let bounds = display.bounds();
        let origin_x = bounds.origin.x;
        let origin_y = bounds.origin.y;
        let lw = bounds.size.width;
        let lh = bounds.size.height;
        if lw <= 0.0 || lh <= 0.0 {
            return Err("目标显示器尺寸无效".to_string());
        }

        let pw = cg_image.width() as f64;
        let scale = if lw > 0.0 { pw / lw } else { 1.0 };

        // SAFETY: cg_image 是 CGDisplay::image()/screenshot 返回的 CGImageRef（Retained）；
        // transmute_copy 把 CGImageRef 按位拷贝为裸指针（CGImageRef 本身是指针，
        // 拷贝指针值不涉及所有权），store_cg_image 内部 Retain 接管
        let raw: *mut std::ffi::c_void = unsafe { std::mem::transmute_copy(&cg_image) };
        store_cg_image(raw);

        store_capture_surface(CaptureSurface {
            display_id: display.id,
            origin_x,
            origin_y,
            width: lw,
            height: lh,
        });

        let windows = enumerate_visible_windows(origin_x, origin_y, lw, lh);

        Ok(ScreenshotData {
            width: lw as u32,
            height: lh as u32,
            scale,
            mouse_x: 0.0,
            mouse_y: 0.0,
            windows,
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("仅支持 macOS".to_string())
    }
}

/// 会话卡死恢复：清 flag / surface / 鼠标跟踪，并尽量关掉 overlay（主线程调用）。
#[cfg(target_os = "macos")]
pub fn abort_screenshot_session(app: &tauri::AppHandle) {
    let _ = exit_impl(app, false);
    // exit_impl 失败路径也保证 flag 可再进
    IS_IN_SCREENSHOT_SESSION.store(false, Ordering::SeqCst);
    clear_capture_surface();
    super::ffi::cancel_picker_jobs();
    super::ffi::store_cg_image(std::ptr::null_mut());
    stop_mouse_tracker();
}

#[cfg(not(target_os = "macos"))]
pub fn abort_screenshot_session(_app: &tauri::AppHandle) {
    IS_IN_SCREENSHOT_SESSION.store(false, Ordering::SeqCst);
}

/// 读取 picker.jpg 并返回 data URL（绕过 asset:// 协议，WKWebView 下 fetch 不可靠）。
#[tauri::command]
pub fn read_picker_image() -> String {
    use base64::Engine;
    match std::fs::read(picker_jpeg_path()) {
        Ok(bytes) => {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            format!("data:image/jpeg;base64,{}", b64)
        }
        Err(_) => String::new(),
    }
}

/// 进入截图模式（仅 Rust 内部：须在主线程调用；不暴露 IPC）。
#[cfg(target_os = "macos")]
pub fn enter_screenshot_mode_sync(app: &tauri::AppHandle, data: ScreenshotData, prev_pid: i32) {
    if let Err(e) = enter_impl(app, &data, prev_pid) {
        clear_capture_surface();
        IS_IN_SCREENSHOT_SESSION.store(false, Ordering::SeqCst);
        eprintln!("进入截图模式失败: {e}");
    }
}
#[cfg(not(target_os = "macos"))]
pub fn enter_screenshot_mode_sync(_app: &tauri::AppHandle, _data: ScreenshotData, _prev_pid: i32) {}

#[cfg(target_os = "macos")]
fn enter_impl(app: &tauri::AppHandle, data: &ScreenshotData, prev_pid: i32) -> Result<(), String> {
    use objc2_app_kit::{
        NSScreen, NSWindow, NSWindowAnimationBehavior, NSWindowCollectionBehavior,
    };
    use objc2_foundation::MainThreadMarker;
    use tauri::Manager;

    let session_gen = SCREENSHOT_GEN.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;

    // prev_pid 由快捷键钩子在 activate_app 前捕获（activate 后 Voidnix 成为 frontmost，
    // current_frontmost_pid 返回 None）。hide_main 的 restore_captured 会 swap 清零
    // PREV_FRONT_PID，故在 hide_main 之后回填。
    crate::runtime::window::hide_main(app);

    crate::platform::focus::capture_pid(prev_pid);

    let surface = capture_surface().unwrap_or(CaptureSurface {
        display_id: 0,
        origin_x: 0.0,
        origin_y: 0.0,
        width: data.width as f64,
        height: data.height as f64,
    });

    // 安全网：万一从非快捷键路径进入（命令直调），确保窗口已创建
    if !super::ensure_screenshot_window(app) {
        return Err("截图窗口创建失败".into());
    }
    let window = app
        .get_webview_window("screenshot")
        .ok_or("找不到截图窗口")?;
    let raw = window
        .ns_window()
        .map_err(|e| e.to_string())?
        .cast::<NSWindow>();
    // SAFETY: ns_window 经 as_ref().ok_or 非空校验；MainThreadMarker 校验主线程；
    // setFrame_display:setAnimationBehavior:setIgnoresMouseEvents:makeKeyAndOrderFront:/
    // windowNumber 均为 NSWindow/NSScreen 标准选择子；cg_image_ptr null 检查后传 FFI
    unsafe {
        let ns_window: &NSWindow = raw.as_ref().ok_or("NSWindow 为空")?;
        let mtm = MainThreadMarker::new().ok_or("不在主线程")?;

        // overlay 几何：与 capture 时 surface.display_id 对齐（禁止用 enter 时点的屏，
        // 否则 capture→enter 间隙移鼠会把 A 屏画面贴到 B 屏 frame）
        let frame = ns_screen_frame_for_display(surface.display_id, mtm)
            .or_else(|| ns_screen_frame_under_cursor(mtm))
            .or_else(|| NSScreen::mainScreen(mtm).map(|s| s.frame()))
            .ok_or("找不到目标屏幕")?;
        ns_window.setFrame_display(frame, true);

        ns_window.setAnimationBehavior(NSWindowAnimationBehavior::None);
        // 多屏独立 Space：CanJoinAllSpaces 降低「在副屏几何正确但 Space 不可见」概率
        let behavior = NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Transient
            | NSWindowCollectionBehavior::IgnoresCycle
            | NSWindowCollectionBehavior::CanJoinAllSpaces;
        ns_window.setCollectionBehavior(behavior);

        let ns_window_ptr = raw.cast::<NSWindow>() as *mut std::ffi::c_void;
        let cg_image_ptr = get_cg_image();
        if !cg_image_ptr.is_null() {
            voidnix_screenshot_set_background(ns_window_ptr, cg_image_ptr);
        }
        // picker JPEG 在 capture 成功后已 start_prepare_picker_jpeg（与 enter 并行）

        let window_number: objc2_foundation::NSInteger = objc2::msg_send![ns_window, windowNumber];
        // Space 迁移必须在 claim key 之前：迁移会打掉 key 状态
        let _ = crate::platform::skylight::move_window_to_active_space(window_number as i64);
        // setFrame 跨屏后二次绑定（部分 macOS 一次不够）
        let _ = crate::platform::skylight::move_window_to_active_space(window_number as i64);
        crate::platform::skylight::set_full_event_shape_for_nswindow(ns_window);

        let ns_window_addr = raw.cast::<NSWindow>() as usize;
        // 立即 opacity=1：contentView.layer.opacity=0 时 WebKit 冷启动会吞首击
        // （表现为重启后前几次截屏必须先点一下，多试几次才好）
        set_window_layer_opacity(ns_window_addr, 1.0);
        let _: () = objc2::msg_send![ns_window, setIgnoresMouseEvents: false];
        ns_window.setAlphaValue(1.0);

        // activate → makeKey → firstResponder=WKWebView
        voidnix_screenshot_claim_key(ns_window_ptr);

        // CGEvent 全局坐标 → 目标屏本地（与 scroll/pin 同源）
        let (gx, gy) = quartz_mouse_location();
        let (mouse_x, mouse_y) = global_to_local(gx, gy, &surface);

        let mut d = data.clone();
        d.mouse_x = mouse_x;
        d.mouse_y = mouse_y;

        if let Ok(json) = serde_json::to_string(&d) {
            let _ = window.eval(format!(
                "window.__screenshotData = {}; window.dispatchEvent(new CustomEvent('__screenshot_ready'));",
                json
            ));
        }

        // setFrame/Space/WebKit 唤醒异步：多拍 claim（含冷启动更长窗口）
        let app_rekey = app.clone();
        let ptr_rekey = ns_window_ptr as usize;
        std::thread::spawn(move || {
            for delay_ms in [0u64, 16, 50, 120, 250] {
                if delay_ms > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                }
                let app_c = app_rekey.clone();
                let _ = app_c.run_on_main_thread(move || {
                    if SCREENSHOT_GEN.load(Ordering::SeqCst) != session_gen {
                        return;
                    }
                    if !IS_IN_SCREENSHOT_SESSION.load(Ordering::SeqCst) {
                        return;
                    }
                    // SAFETY: ptr 为 enter 时缓存的 NSWindow 地址，会话 gen 已校验
                    voidnix_screenshot_claim_key(ptr_rekey as *mut std::ffi::c_void);
                });
            }
        });
    }
    start_mouse_tracker(app);
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn enter_impl(_app: &tauri::AppHandle, _data: &ScreenshotData) -> Result<(), String> {
    Err("仅支持 macOS".to_string())
}

#[tauri::command]
pub async fn screenshot_overlay_ready(app: tauri::AppHandle) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
    let app_c = app.clone();
    app.run_on_main_thread(move || {
        let _ = tx.send(overlay_ready_impl(&app_c));
    })
    .map_err(|e| e.to_string())?;
    rx.recv_timeout(std::time::Duration::from_secs(10))
        .map_err(|e| e.to_string())?
}

#[cfg(target_os = "macos")]
fn overlay_ready_impl(app: &tauri::AppHandle) -> Result<(), String> {
    use objc2_app_kit::NSWindow;
    use tauri::Manager;
    let window = app
        .get_webview_window("screenshot")
        .ok_or("找不到截图窗口")?;
    let raw = window
        .ns_window()
        .map_err(|e| e.to_string())?
        .cast::<NSWindow>();
    let ns_window_ptr = raw.cast::<NSWindow>() as *mut std::ffi::c_void;
    let ns_window_addr = raw.cast::<NSWindow>() as usize;
    // 前端挂载完成时再 claim（Vue mount 后 firstResponder 才稳）
    // SAFETY: ptr 来自合法 NSWindow
    unsafe {
        voidnix_screenshot_claim_key(ns_window_ptr);
    }
    set_window_layer_opacity(ns_window_addr, 1.0);
    let _ = window.eval(
        "window.dispatchEvent(new Event('focus')); document.querySelector('[tabindex]')?.focus?.()",
    );
    Ok(())
}

/// 启动后预热截屏 WebView（不抢焦点）。
#[cfg(target_os = "macos")]
pub fn prewarm_screenshot_window(app: &tauri::AppHandle) {
    use objc2_app_kit::NSWindow;
    use tauri::Manager;
    let Some(window) = app.get_webview_window("screenshot") else {
        return;
    };
    let Ok(raw) = window.ns_window().map(|p| p.cast::<NSWindow>()) else {
        return;
    };
    let ptr = raw.cast::<NSWindow>() as *mut std::ffi::c_void;
    // SAFETY: 合法 NSWindow；prewarm 不 activate
    unsafe {
        voidnix_screenshot_prewarm(ptr);
    }
    // 轻触 JS，拉起 WebContent 进程
    let _ = window.eval("void 0");
}

#[cfg(not(target_os = "macos"))]
pub fn prewarm_screenshot_window(_app: &tauri::AppHandle) {}

#[cfg(not(target_os = "macos"))]
fn overlay_ready_impl(_app: &tauri::AppHandle) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub async fn exit_screenshot_mode(
    app: tauri::AppHandle,
    no_restore_focus: Option<bool>,
) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
    let app_c = app.clone();
    app.run_on_main_thread(move || {
        let _ = tx.send(exit_impl(&app_c, no_restore_focus.unwrap_or(false)));
    })
    .map_err(|e| e.to_string())?;
    rx.recv_timeout(std::time::Duration::from_secs(10))
        .map_err(|e| e.to_string())?
}

#[cfg(target_os = "macos")]
fn exit_impl(app: &tauri::AppHandle, no_restore_focus: bool) -> Result<(), String> {
    use objc2_app_kit::{
        NSApplicationActivationOptions, NSWindow, NSWindowAnimationBehavior, NSWorkspace,
    };

    stop_mouse_tracker();
    IS_IN_SCREENSHOT_SESSION.store(false, std::sync::atomic::Ordering::SeqCst);
    clear_capture_surface();

    let raw = crate::extensions::screenshot::screenshot_ns_window(app).ok_or("找不到截图窗口")?;
    let session_gen = SCREENSHOT_GEN.load(std::sync::atomic::Ordering::SeqCst);
    let ns_window_addr = raw as usize;
    // SAFETY: ns_window 经 as_ref().ok_or 非空校验；setAnimationBehavior/
    // setIgnoresMouseEvents:/resignKeyWindow 均为 NSWindow 标准选择子
    unsafe {
        let ns_window: &NSWindow = raw.as_ref().ok_or("NSWindow 为空")?;
        ns_window.setAnimationBehavior(NSWindowAnimationBehavior::None);
        let _: () = objc2::msg_send![ns_window, setIgnoresMouseEvents: true];
        let _: () = objc2::msg_send![ns_window, resignKeyWindow];
    }

    fade_window_layer_opacity(
        ns_window_addr,
        0.0,
        0.15,
        Some(Box::new(move || {
            if SCREENSHOT_GEN.load(std::sync::atomic::Ordering::SeqCst) == session_gen {
                // SAFETY: ns_window_addr 由调用方传入合法 NSWindow 地址；session_gen 校验
                // 确认窗口未被重新进入（避免操作过期窗口）；clear_background 为 FFI 薄壳，
                // setAlphaValue: 为 NSWindow 标准选择子
                unsafe {
                    let ptr = ns_window_addr as *mut std::ffi::c_void;
                    voidnix_screenshot_clear_background(ptr);
                    let nsw = ns_window_addr as *mut objc2::runtime::AnyObject;
                    let _: () = objc2::msg_send![nsw, setAlphaValue: 0.0_f64];
                }
            }
        })),
    );

    let prev_pid = crate::platform::focus::take_captured_pid();
    // 始终保存 prev_pid 给 pin 窗口使用（即使 noRestoreFocus=true）
    #[cfg(target_os = "macos")]
    {
        super::pin::PIN_PREV_PID.store(prev_pid, Ordering::SeqCst);
    }
    if !no_restore_focus && prev_pid > 0 {
        // deactivate 广播 NSApplicationWillResignActiveNotification，配合
        // activateWithOptions 把 system key / first responder 完整还给原应用；
        // 缺此步 macOS 可能不转移 first responder（光标不回原输入框）。
        crate::platform::focus::deactivate_app();
        let ws = NSWorkspace::sharedWorkspace();
        if let Some(target) = ws
            .runningApplications()
            .iter()
            .find(|a| a.processIdentifier() == prev_pid)
        {
            #[allow(deprecated)]
            target.activateWithOptions(NSApplicationActivationOptions::ActivateIgnoringOtherApps);
        }
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn exit_impl(_app: &tauri::AppHandle, _no_restore_focus: bool) -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn reactivate_screenshot_window(app: &tauri::AppHandle) {
    use objc2_app_kit::NSWindow;
    use tauri::Manager;
    // pin 窗口存在时跳过重激活：create_pin_webview 的 activate_app 触发本观察器，
    // 此时截图窗口正在退出（exit 与 pin 并发），window alpha 尚为 1.0（fade 只改
    // layer opacity），makeKeyAndOrderFront 会把截图窗口拉到钉图窗口前面遮住它。
    if app.webview_windows().keys().any(|l| l.starts_with("pin-")) {
        return;
    }
    let Some(window) = app.get_webview_window("screenshot") else {
        return;
    };
    let Ok(raw) = window.ns_window().map(|p| p.cast::<NSWindow>()) else {
        return;
    };
    // SAFETY: raw 来自 ns_window() Ok 分支；as_ref 返回 Option 经 let Some 解构非空校验
    let Some(ns_window) = (unsafe { raw.as_ref() }) else {
        return;
    };
    if ns_window.alphaValue() < 0.5 {
        return;
    }
    // SAFETY: isOnActiveSpace 为 NSWindow 标准选择子（返回 bool）
    let is_on_active_space: bool = unsafe { objc2::msg_send![ns_window, isOnActiveSpace] };
    if !is_on_active_space {
        return;
    }
    // SAFETY: ns_window 已上方非空校验；makeKeyAndOrderFront: 为 NSWindow 标准选择子，
    // 参数为 null（nil sender）
    unsafe {
        let _: () = objc2::msg_send![
            ns_window,
            makeKeyAndOrderFront: std::ptr::null::<objc2::runtime::AnyObject>()
        ];
    }
    crate::platform::focus::activate_app();
    let _ = window.eval("window.dispatchEvent(new Event('focus'))");
}
