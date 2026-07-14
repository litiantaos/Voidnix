use std::sync::atomic::Ordering;

use super::ffi::{
    get_cg_image, picker_jpeg_path, prepare_picker_jpeg, store_cg_image,
    voidnix_screenshot_clear_background, voidnix_screenshot_set_background, ScreenshotData,
    WindowRect,
};

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
                    if let Some(f) = slot_clone.lock().unwrap_or_else(|e| e.into_inner()).take() {
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
    use objc2::runtime::AnyObject;
    use std::sync::Mutex;
    use tauri::Manager;

    struct SendObj(*mut AnyObject);
    unsafe impl Send for SendObj {}
    unsafe impl Sync for SendObj {}

    static GLOBAL_MONITOR: Mutex<SendObj> = Mutex::new(SendObj(std::ptr::null_mut()));
    static LOCAL_MONITOR: Mutex<SendObj> = Mutex::new(SendObj(std::ptr::null_mut()));

    pub fn start(app: &tauri::AppHandle) {
        use objc2::ClassType;
        use objc2_app_kit::{NSEvent, NSScreen};
        use objc2_foundation::MainThreadMarker;
        {
            let g = GLOBAL_MONITOR.lock().unwrap_or_else(|e| e.into_inner());
            if !g.0.is_null() {
                return;
            }
        }
        let screen_h = {
            let mtm = match MainThreadMarker::new() {
                Some(m) => m,
                None => return,
            };
            NSScreen::mainScreen(mtm)
                .map(|s| s.frame().size.height)
                .unwrap_or(0.0)
        };
        let mask: u64 = (1u64 << 5) | (1u64 << 6) | (1u64 << 7) | (1u64 << 27);
        {
            let app = app.clone();
            let blk = block2::RcBlock::new(move |_event: *mut AnyObject| {
                emit_mouse(&app, screen_h);
            });
            // SAFETY: mask 为标准 NSEvent 位掩码；block 经 RcBlock 持有，&*block 取引用；
            // addGlobalMonitorForEventsMatchingMask: 返回 monitor 对象，null 检查后
            // retain + forget block（生命周期随 monitor，stop 时 removeMonitor+release）
            unsafe {
                let m: *mut AnyObject = objc2::msg_send![NSEvent::class(), addGlobalMonitorForEventsMatchingMask: mask, handler: &*blk];
                if !m.is_null() {
                    let _: () = objc2::msg_send![m, retain];
                    *GLOBAL_MONITOR.lock().unwrap_or_else(|e| e.into_inner()) = SendObj(m);
                    std::mem::forget(blk);
                }
            }
        }
        {
            let app = app.clone();
            let blk = block2::RcBlock::new(move |event: *mut AnyObject| -> *mut AnyObject {
                emit_mouse(&app, screen_h);
                event
            });
            // SAFETY: 同上 global monitor 契约；local monitor 额外返回 event（透传）
            unsafe {
                let m: *mut AnyObject = objc2::msg_send![NSEvent::class(), addLocalMonitorForEventsMatchingMask: mask, handler: &*blk];
                if !m.is_null() {
                    let _: () = objc2::msg_send![m, retain];
                    *LOCAL_MONITOR.lock().unwrap_or_else(|e| e.into_inner()) = SendObj(m);
                    std::mem::forget(blk);
                }
            }
        }
    }

    pub fn stop() {
        use objc2::ClassType;
        use objc2_app_kit::NSEvent;
        for slot in [&GLOBAL_MONITOR, &LOCAL_MONITOR] {
            let mut g = slot.lock().unwrap_or_else(|e| e.into_inner());
            if !g.0.is_null() {
                // SAFETY: g.0 非 null（已检查）；removeMonitor + release 与 start 的
                // retain + forget 配对（monitor 注销 + 引用计数归零）
                unsafe {
                    let _: () = objc2::msg_send![NSEvent::class(), removeMonitor: g.0];
                    let _: () = objc2::msg_send![g.0, release];
                }
                *g = SendObj(std::ptr::null_mut());
            }
        }
    }

    fn emit_mouse(app: &tauri::AppHandle, screen_h: f64) {
        use objc2_app_kit::{NSEvent, NSWindow};
        let Some(window) = app.get_webview_window("screenshot") else {
            return;
        };
        let loc = NSEvent::mouseLocation();
        let cx = loc.x;
        let cy = screen_h - loc.y;
        let _ = window.eval(format!(
            "window.__setScreenshotCross && window.__setScreenshotCross({},{})",
            cx, cy
        ));
        if let Ok(raw) = window.ns_window().map(|p| p.cast::<NSWindow>()) {
            // SAFETY: ns 经 as_ref Some 分支非空校验；isOnActiveSpace/alphaValue/isKeyWindow/
            // makeKeyAndOrderFront: 均为 NSWindow 标准选择子，参数类型匹配
            unsafe {
                if let Some(ns) = raw.as_ref() {
                    let on_space: bool = objc2::msg_send![ns, isOnActiveSpace];
                    if ns.alphaValue() > 0.5 && !ns.isKeyWindow() && on_space {
                        let _: () = objc2::msg_send![ns, makeKeyAndOrderFront: std::ptr::null::<AnyObject>()];
                        crate::platform::focus::activate_app();
                    }
                }
            }
        }
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
fn enumerate_visible_windows() -> Vec<WindowRect> {
    use core_foundation::array::{CFArray, CFArrayRef};
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use std::ffi::c_void;

    type CGWindowListOption = u32;
    const CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY: CGWindowListOption = 1 << 0;
    const CG_WINDOW_LIST_EXCLUDE_DESKTOP_ELEMENTS: CGWindowListOption = 1 << 4;
    type CGWindowID = u32;
    extern "C" {
        fn CGWindowListCopyWindowInfo(
            option: CGWindowListOption,
            relativeToWindow: CGWindowID,
        ) -> CFArrayRef;
    }

    // SAFETY: CGWindowListCopyWindowInfo 为 CoreGraphics C API，option 为合法位掩码，
    // relativeToWindow=0（无相对窗口）；返回 Create 规则 CFArray，null 检查后由
    // wrap_under_create_rule 接管所有权
    let raw = unsafe {
        CGWindowListCopyWindowInfo(
            CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY | CG_WINDOW_LIST_EXCLUDE_DESKTOP_ELEMENTS,
            0,
        )
    };
    if raw.is_null() {
        return Vec::new();
    }
    let array: CFArray<CFDictionary<*const c_void, *const c_void>> =
        // SAFETY: raw 由 CGWindowListCopyWindowInfo 返回（Create 规则，已 null 检查），
        // wrap_under_create_rule 接管所有权
        unsafe { CFArray::wrap_under_create_rule(raw) };

    let self_pid = std::process::id() as i64;
    let mut result = Vec::with_capacity(array.len() as usize);

    let key_layer = CFString::from_static_string("kCGWindowLayer");
    let key_bounds = CFString::from_static_string("kCGWindowBounds");
    let key_pid = CFString::from_static_string("kCGWindowOwnerPID");
    let key_name = CFString::from_static_string("kCGWindowOwnerName");
    let key_alpha = CFString::from_static_string("kCGWindowAlpha");

    let lookup =
        |dict: &CFDictionary<*const c_void, *const c_void>, key: &CFString| -> Option<CFType> {
            let ptr = key.as_concrete_TypeRef() as *const c_void;
            let v = dict.find(ptr)?;
            if v.is_null() {
                return None;
            }
            // SAFETY: *v 已非空校验；wrap_under_get_rule 遵循 CF Get 规则（不获取所有权）
            Some(unsafe { CFType::wrap_under_get_rule(*v as _) })
        };

    for i in 0..array.len() {
        let Some(dict) = array.get(i) else {
            continue;
        };

        let layer = lookup(&dict, &key_layer)
            .and_then(|v| v.downcast::<CFNumber>())
            .and_then(|n| n.to_i64())
            .unwrap_or(-1);
        // 仅纳入普通内容窗口层级 [0, kCGMainMenuWindowLevel)：
        // 覆盖 Normal(0)/Floating(3, Quick Look 等)/ModalPanel(8)/Utility(19)，
        // 排除菜单栏(24)/Status(25)/PopUpMenu(101)/Overlay(102)/Help(200)/Cursor 等 chrome；
        // Dock/桌面由 ExcludeDesktopElements 处理，自身 overlay 由 pid 排除
        const MAIN_MENU_LEVEL: i64 = 24;
        if !(0..MAIN_MENU_LEVEL).contains(&layer) {
            continue;
        }

        let pid = lookup(&dict, &key_pid)
            .and_then(|v| v.downcast::<CFNumber>())
            .and_then(|n| n.to_i64())
            .unwrap_or(0);
        if pid == self_pid {
            continue;
        }

        let alpha = lookup(&dict, &key_alpha)
            .and_then(|v| v.downcast::<CFNumber>())
            .and_then(|n| n.to_f64())
            .unwrap_or(1.0);
        if alpha < 0.05 {
            continue;
        }

        let owner = lookup(&dict, &key_name)
            .and_then(|v| v.downcast::<CFString>())
            .map(|s| s.to_string())
            .unwrap_or_default();

        let Some(bd) = lookup(&dict, &key_bounds)
            .and_then(|v| v.downcast::<CFDictionary<*const c_void, *const c_void>>())
        else {
            continue;
        };

        let gn = |k: &'static str| -> f64 {
            let ks = CFString::from_static_string(k);
            lookup(&bd, &ks)
                .and_then(|v| v.downcast::<CFNumber>())
                .and_then(|n| n.to_f64())
                .unwrap_or(0.0)
        };
        let (x, y, w, h) = (gn("X"), gn("Y"), gn("Width"), gn("Height"));
        if w < 40.0 || h < 40.0 {
            continue;
        }
        result.push(WindowRect { x, y, w, h, owner });
    }
    result
}

#[cfg(not(target_os = "macos"))]
fn enumerate_visible_windows() -> Vec<WindowRect> {
    Vec::new()
}

#[cfg(target_os = "macos")]
static SCREENSHOT_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(target_os = "macos")]
pub(super) static IS_IN_SCREENSHOT_SESSION: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// 捕获主屏（仅 Rust 内部：快捷键路径 → enter_screenshot_mode_sync；不暴露 IPC）。
pub fn capture_screen() -> Result<ScreenshotData, String> {
    use core_graphics::display::CGDisplay;

    let display = CGDisplay::main();
    let bounds = display.bounds();
    let lw = bounds.size.width as u32;
    let lh = bounds.size.height as u32;

    let cg_image = display
        .image()
        .ok_or("CGDisplayCreateImage 失败：请在「系统设置 → 隐私与安全性 → 屏幕录制」中授权")?;

    let pw = cg_image.width() as u32;
    let scale = if lw > 0 { pw as f64 / lw as f64 } else { 1.0 };

    #[cfg(target_os = "macos")]
    {
        // SAFETY: cg_image 是 CGDisplay::image() 返回的 CGImageRef（Retained）；
        // transmute_copy 把 CGImageRef 按位拷贝为裸指针（CGImageRef 本身是指针，
        // 拷贝指针值不涉及所有权），store_cg_image 内部 Retain 接管
        let raw: *mut std::ffi::c_void = unsafe { std::mem::transmute_copy(&cg_image) };
        store_cg_image(raw);
    }

    let windows = enumerate_visible_windows();

    Ok(ScreenshotData {
        width: lw,
        height: lh,
        scale,
        mouse_x: 0.0,
        mouse_y: 0.0,
        windows,
    })
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
pub fn enter_screenshot_mode_sync(app: &tauri::AppHandle, data: ScreenshotData) {
    let _ = enter_impl(app, &data);
}
#[cfg(not(target_os = "macos"))]
pub fn enter_screenshot_mode_sync(_app: &tauri::AppHandle, _data: ScreenshotData) {}

#[cfg(target_os = "macos")]
fn enter_impl(app: &tauri::AppHandle, data: &ScreenshotData) -> Result<(), String> {
    use objc2_app_kit::{NSEvent, NSScreen, NSWindow, NSWindowAnimationBehavior};
    use objc2_foundation::MainThreadMarker;
    use tauri::Manager;

    SCREENSHOT_GEN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

    // 先取原前台 pid（必须在 hide_main 前读取才是"截图前的应用"）。
    // hide_main 会 clear focus 唯一源（restore_captured swap），故在 hide_main
    // 之后回填到唯一源。
    let prev_pid = crate::platform::focus::current_frontmost_pid().unwrap_or(0);

    crate::runtime::window::hide_main(app);

    crate::platform::focus::capture_pid(prev_pid);

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

        let screen_height = if let Some(screen) = NSScreen::mainScreen(mtm) {
            let frame = screen.frame();
            ns_window.setFrame_display(frame, true);
            frame.size.height
        } else {
            data.height as f64
        };

        ns_window.setAnimationBehavior(NSWindowAnimationBehavior::None);

        let ns_window_ptr = raw.cast::<NSWindow>() as *mut std::ffi::c_void;
        let cg_image_ptr = get_cg_image();
        if !cg_image_ptr.is_null() {
            voidnix_screenshot_set_background(ns_window_ptr, cg_image_ptr);
        }

        let _ = std::fs::remove_file(picker_jpeg_path());
        if !cg_image_ptr.is_null() {
            prepare_picker_jpeg(cg_image_ptr);
        }

        let window_number: objc2_foundation::NSInteger = objc2::msg_send![ns_window, windowNumber];
        let _ = crate::platform::skylight::move_window_to_active_space(window_number as i64);

        let ns_window_addr = raw.cast::<NSWindow>() as usize;
        set_window_layer_opacity(ns_window_addr, 0.0);
        let _: () = objc2::msg_send![ns_window, setIgnoresMouseEvents: false];
        ns_window.setAlphaValue(1.0);
        let _: () = objc2::msg_send![
            ns_window,
            makeKeyAndOrderFront: std::ptr::null::<objc2::runtime::AnyObject>()
        ];

        crate::platform::focus::activate_app();

        let mouse_loc = NSEvent::mouseLocation();
        let mouse_x = mouse_loc.x;
        let mouse_y = screen_height - mouse_loc.y;

        let mut d = data.clone();
        d.mouse_x = mouse_x;
        d.mouse_y = mouse_y;

        if let Ok(json) = serde_json::to_string(&d) {
            let _ = window.eval(format!(
                "window.__screenshotData = {}; window.dispatchEvent(new CustomEvent('__screenshot_ready'));",
                json
            ));
        }
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
    let ns_window_addr = raw.cast::<NSWindow>() as usize;
    fade_window_layer_opacity(ns_window_addr, 1.0, 0.18, None);
    Ok(())
}

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
    use tauri::Manager;

    stop_mouse_tracker();
    IS_IN_SCREENSHOT_SESSION.store(false, std::sync::atomic::Ordering::SeqCst);

    let window = app
        .get_webview_window("screenshot")
        .ok_or("找不到截图窗口")?;
    let raw = window
        .ns_window()
        .map_err(|e| e.to_string())?
        .cast::<NSWindow>();
    let session_gen = SCREENSHOT_GEN.load(std::sync::atomic::Ordering::SeqCst);
    let ns_window_addr = raw.cast::<NSWindow>() as usize;
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
