//! 清洁模式扩展：全屏黑窗 + 键鼠锁定（CGEventTap）+ 长按左键 2s 退出。
//!
//! tap 注册到 **main run loop + kCFRunLoopCommonModes**（参照 Hammerspoon /
//! Scroll Reverser / gltchitm-keyboard-cleaner，千万级用户验证的最稳组合——
//! 不挂独立线程 / 独立 run loop；default mode 会在 event-tracking / modal 切换时
//! 饿死 source 导致 tap 被系统判定慢而静默禁用，common modes 覆盖所有 mode）。
//!
//! 退出：长按鼠标 / 触控板左键 2s（**CGEventTap callback + NSView mouseDown
//! 双路检测**，共用 `LEFT_DOWN_AT` 原子时间戳——tap 正常时 callback 设值，
//! tap 静默失效时事件穿透到 NSView 由 mouseDown 设值，任一失效另一路兜底）。
//! 满足条件或 180s 兜底 → **std::process::exit(0)**（不依赖主线程派发，主线程
//! 卡死也能退出——这是「被困黑屏只能重启」的根因修复）。进程退出后系统自动
//! 回收（窗口消失 / 光标解冻 / 键盘解锁），下次启动状态自然重置。
//!
//! 光标冻结：`CGAssociateMouseAndMouseCursorPosition(0)` 补充 tap 吞不掉的
//! 光标位移（tap 只能吞事件，光标位置由 WindowServer 直接更新）。

use crate::runtime::registry::Extension;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
use objc2_foundation::{NSPoint, NSRect, NSString};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

// ============================================================================
// FFI
// ============================================================================

#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

extern "C" {
    // objc runtime（NSView 子类化 isa-swizzling，与 platform/panel.rs 同范式）
    fn objc_getClass(name: *const c_void) -> *mut c_void;
    fn objc_allocateClassPair(
        superclass: *const c_void,
        name: *const c_void,
        extra_bytes: usize,
    ) -> *mut c_void;
    fn objc_registerClassPair(cls: *mut c_void);
    fn class_addMethod(
        cls: *const c_void,
        name: *const c_void,
        imp: *const c_void,
        types: *const c_void,
    ) -> i32;
    fn sel_registerName(name: *const c_void) -> *const c_void;
    fn object_setClass(obj: *mut c_void, cls: *const c_void) -> *mut c_void;
    // CGEventTap
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events: u64,
        cb: extern "C" fn(*mut c_void, u32, *mut c_void, *mut c_void) -> *mut c_void,
        user_info: *mut c_void,
    ) -> *mut c_void;
    fn CGEventTapEnable(tap: *mut c_void, enable: u32);
    fn CGEventTapIsEnabled(tap: *mut c_void) -> u32;
    fn CGEventSetFlags(event: *mut c_void, flags: u64);
    fn CGAssociateMouseAndMouseCursorPosition(active: u32) -> i32;
    fn CGDisplayHideCursor(display: u32) -> i32;
    fn CGDisplayShowCursor(display: u32) -> i32;
    fn CGMainDisplayID() -> u32;
    fn CGDisplayPixelsWide(display: u32) -> usize;
    fn CGDisplayPixelsHigh(display: u32) -> usize;
    fn CGWarpMouseCursorPosition(pt: CGPoint) -> i32;
    // CoreFoundation run loop（main run loop，不自己跑）
    fn CFRunLoopGetMain() -> *mut c_void;
    fn CFMachPortCreateRunLoopSource(
        alloc: *const c_void,
        port: *mut c_void,
        order: isize,
    ) -> *mut c_void;
    fn CFRunLoopAddSource(rl: *mut c_void, source: *mut c_void, mode: *const c_void);
    fn CFRunLoopRemoveSource(rl: *mut c_void, source: *mut c_void, mode: *const c_void);
    fn CFRelease(cf: *mut c_void);
    static kCFRunLoopCommonModes: *const c_void;
}

// ============================================================================
// 常量
// ============================================================================

const HOLD_MS: u64 = 2000; // 长按退出阈值
const POLL_MS: u64 = 100; // 长按检测精度
const WATCHDOG_MS: u64 = 2000; // tap 健康检查间隔

const HID_TAP: u32 = 2; // kCGHIDEventTap — HID 层，覆盖媒体键 / 功能键 / 亮度键
const HEAD_INSERT: u32 = 0; // kCGHeadInsertEventTap — 最先收到，先于其他 tap 吞
const TAP_DEFAULT: u32 = 0; // kCGEventTapOptionDefault — 可 suppress

/// 全量事件 mask（kCGEventMaskForAllEvents）— callback 收到一切，按类型选择放行 / 吞。
const MASK_ALL: u64 = u64::MAX;

const EVT_LEFT_DOWN: u32 = 1; // kCGEventLeftMouseDown
const EVT_LEFT_UP: u32 = 2; // kCGEventLeftMouseUp
const EVT_MOUSE_MOVED: u32 = 5; // kCGEventMouseMoved
const EVT_FLAGS_CHANGED: u32 = 12; // kCGEventFlagsChanged
const EVT_TAP_DISABLED_TIMEOUT: u32 = 0xFFFF_FFFE; // kCGEventTapDisabledByTimeout
const EVT_TAP_DISABLED_USER_INPUT: u32 = 0xFFFF_FFFF; // kCGEventTapDisabledByUserInput

const SCREEN_SAVER_LEVEL: i64 = 1000; // NSScreenSaverWindowLevel — 非屏保应用可用的最高层
/// CanJoinAllSpaces(1<<0) | FullScreenAuxiliary(1<<8) — 覆盖全屏应用 Space
const COLLECTION_BEHAVIOR: usize = 1 | (1 << 8);

/// 黑窗不透明度（测试用半透明调 0.8，正式 1.0 全黑）
const BLACK_ALPHA: f64 = 1.0;

// ============================================================================
// 全局状态
// ============================================================================

struct CleanState {
    windows: Vec<*mut AnyObject>,
    active: bool,
}
unsafe impl Send for CleanState {}

static STATE: Mutex<CleanState> = Mutex::new(CleanState {
    windows: Vec::new(),
    active: false,
});

/// 左键按下时刻（ms since epoch），0 = 未按下。callback + NSView 双路写入，
/// poll 线程轮询判断长按。
static LEFT_DOWN_AT: AtomicU64 = AtomicU64::new(0);

/// CGEventTap 句柄（callback 内重启 + watchdog 健康检查用）。
static TAP_REF: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

/// CFRunLoopSource 句柄（stop 时 remove + release 用）。
static TAP_SOURCE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

static POLL_STOP: AtomicBool = AtomicBool::new(true);

/// poll 线程检测到长按达标后，通过此 handle 派发 disable 到主线程。
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

// ============================================================================
// NSView 子类化（mouseDown / mouseUp → 设 / 清 LEFT_DOWN_AT）
// 与 platform/panel.rs 同范式（isa-swizzling + objc runtime）
// ============================================================================

struct CleanViewClass(*mut c_void);
unsafe impl Send for CleanViewClass {}
unsafe impl Sync for CleanViewClass {}

static CLEAN_VIEW_CLASS: OnceLock<CleanViewClass> = OnceLock::new();

extern "C" fn clean_mouse_down(_this: *mut c_void, _cmd: *const c_void, _event: *mut c_void) {
    LEFT_DOWN_AT.store(now_ms(), Ordering::Relaxed);
}

extern "C" fn clean_mouse_up(_this: *mut c_void, _cmd: *const c_void, _event: *mut c_void) {
    LEFT_DOWN_AT.store(0, Ordering::Relaxed);
}

fn ensure_clean_view_class() -> *mut c_void {
    let CleanViewClass(ptr) = *CLEAN_VIEW_CLASS.get_or_init(|| unsafe {
        let name = c"VoidnixCleanView".as_ptr() as *const c_void;
        let existing = objc_getClass(name);
        if !existing.is_null() {
            return CleanViewClass(existing);
        }
        let superclass = objc_getClass(c"NSView".as_ptr() as *const c_void);
        let cls = objc_allocateClassPair(superclass, name, 0);
        assert!(!cls.is_null(), "objc_allocateClassPair failed");
        let types = c"v@:@".as_ptr() as *const c_void;
        for (sel_name, imp) in [
            (c"mouseDown:".as_ptr(), clean_mouse_down as *const c_void),
            (c"mouseUp:".as_ptr(), clean_mouse_up as *const c_void),
        ] {
            let sel = sel_registerName(sel_name as *const c_void);
            let added = class_addMethod(cls, sel, imp, types);
            debug_assert!(added >= 0);
        }
        objc_registerClassPair(cls);
        CleanViewClass(cls)
    });
    ptr
}

/// 在 contentView 居中添加浅色退出提示文字。
fn add_hint_label(view: *mut AnyObject, screen_frame: NSRect) {
    unsafe {
        let text = NSString::from_str("长按鼠标 / 触控板 2 秒退出");
        let label: *mut AnyObject = msg_send![class!(NSTextField), labelWithString: &*text];
        let faint: *mut AnyObject =
            msg_send![class!(NSColor), colorWithWhite: 1.0f64, alpha: 0.2f64];
        let _: () = msg_send![label, setTextColor: faint];
        let font: *mut AnyObject = msg_send![class!(NSFont), systemFontOfSize: 12.0f64];
        let _: () = msg_send![label, setFont: font];
        let _: () = msg_send![label, sizeToFit];
        let lbl: NSRect = msg_send![label, frame];
        let origin = NSPoint {
            x: (screen_frame.size.width - lbl.size.width) / 2.0,
            y: (screen_frame.size.height - lbl.size.height) / 2.0,
        };
        let _: () = msg_send![label, setFrameOrigin: origin];
        let _: () = msg_send![view, addSubview: label];
    }
}

// ============================================================================
// CGEventTap callback（放行左键 + 鼠标移动，吞其余一切）
// ============================================================================

extern "C" fn event_tap_callback(
    _proxy: *mut c_void,
    event_type: u32,
    event: *mut c_void,
    _user_info: *mut c_void,
) -> *mut c_void {
    // tap 被系统禁用（超时 / 用户输入）→ 重启（Hammerspoon 范式）
    if event_type == EVT_TAP_DISABLED_TIMEOUT || event_type == EVT_TAP_DISABLED_USER_INPUT {
        let tap = TAP_REF.load(Ordering::Relaxed);
        if !tap.is_null() {
            unsafe { CGEventTapEnable(tap, 1) };
        }
        return event;
    }
    // 放行左键按下 / 释放（长按检测）+ 鼠标移动（光标靠 CGAssociate 冻结，
    // 吞掉 MouseMoved 会被系统强制重新关联光标，反而失效）。
    match event_type {
        EVT_LEFT_DOWN => {
            LEFT_DOWN_AT.store(now_ms(), Ordering::Relaxed);
            event
        }
        EVT_LEFT_UP => {
            LEFT_DOWN_AT.store(0, Ordering::Relaxed);
            event
        }
        EVT_MOUSE_MOVED => event,
        EVT_FLAGS_CHANGED => {
            // 修饰键：清零 flags 后放行（吞掉 FlagsChanged 不能阻止 HID 驱动
            // 更新 modifier state，清零让系统认为无修饰键按下）
            unsafe { CGEventSetFlags(event, 0) };
            event
        }
        // 吞掉一切：键盘全键（含媒体键 / 功能键）+ 右键 + 滚轮 + 触控板手势 + 拖拽。
        // 固件级按键（fn 切换 / 语音键 / Caps Lock LED）由键盘固件直接处理，
        // 不经过 HID 事件，CGEventTap 拦不住——属 macOS 硬件限制。
        _ => std::ptr::null_mut(),
    }
}

// ============================================================================
// tap 生命周期（main run loop + common modes，无独立线程）
// ============================================================================

fn start_keyboard_tap() -> bool {
    unsafe {
        let tap = CGEventTapCreate(
            HID_TAP,
            HEAD_INSERT,
            TAP_DEFAULT,
            MASK_ALL,
            event_tap_callback,
            std::ptr::null_mut(),
        );
        if tap.is_null() {
            eprintln!("[clean-mode] CGEventTapCreate 失败（辅助功能权限不足）");
            return false;
        }
        TAP_REF.store(tap, Ordering::Relaxed);

        let source = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
        if source.is_null() {
            CFRelease(tap);
            TAP_REF.store(std::ptr::null_mut(), Ordering::Relaxed);
            return false;
        }
        CFRunLoopAddSource(CFRunLoopGetMain(), source, kCFRunLoopCommonModes);
        TAP_SOURCE.store(source, Ordering::Relaxed);
        true
    }
}

fn stop_keyboard_tap() {
    let source = TAP_SOURCE.swap(std::ptr::null_mut(), Ordering::Relaxed);
    let tap = TAP_REF.swap(std::ptr::null_mut(), Ordering::Relaxed);
    if source.is_null() && tap.is_null() {
        return;
    }
    unsafe {
        if !source.is_null() {
            CFRunLoopRemoveSource(CFRunLoopGetMain(), source, kCFRunLoopCommonModes);
            CFRelease(source);
        }
        if !tap.is_null() {
            CFRelease(tap);
        }
    }
}

// ============================================================================
// 开启 / 关闭
// ============================================================================

fn enable_clean_mode() -> Result<(), String> {
    unsafe {
        let screens: *mut AnyObject = msg_send![class!(NSScreen), screens];
        let count: usize = msg_send![screens, count];
        let black: *mut AnyObject = msg_send![class!(NSColor), colorWithSRGBRed: 0.0f64, green: 0.0f64, blue: 0.0f64, alpha: BLACK_ALPHA];
        let view_class = ensure_clean_view_class();
        let mut windows = Vec::with_capacity(count);

        for i in 0..count {
            let screen: *mut AnyObject = msg_send![screens, objectAtIndex: i];
            let frame: NSRect = msg_send![screen, frame];
            let alloc: *mut AnyObject = msg_send![class!(NSWindow), alloc];
            let window: *mut AnyObject = msg_send![
                alloc,
                initWithContentRect: frame,
                styleMask: 0usize, // borderless
                backing: 2usize,   // NSBackingStoreBuffered
                defer: false
            ];
            if window.is_null() {
                continue;
            }
            let _: () = msg_send![window, setBackgroundColor: black];
            let _: () = msg_send![window, setOpaque: false];
            let _: () = msg_send![window, setLevel: SCREEN_SAVER_LEVEL];
            let _: () = msg_send![window, setCollectionBehavior: COLLECTION_BEHAVIOR];
            let _: () = msg_send![window, setHidesOnDeactivate: false];
            let _: () = msg_send![window, setHasShadow: false];
            let _: () = msg_send![window, setReleasedWhenClosed: false];

            // contentView isa-swizzle → 自定义 mouseDown / mouseUp（长按检测第二路）
            let view: *mut AnyObject = msg_send![class!(NSView), alloc];
            let view: *mut AnyObject = msg_send![view, initWithFrame: frame];
            if !view.is_null() {
                object_setClass(view as *mut c_void, view_class);
                add_hint_label(view, frame);
                let _: () = msg_send![window, setContentView: view];
            }

            let _: () = msg_send![window, orderFrontRegardless];
            windows.push(window);
        }

        // CGEventTap（锁键盘 + 长按检测第一路）。失败则清理窗口并报错引导授权。
        if !start_keyboard_tap() {
            for w in &windows {
                let _: () = msg_send![*w, orderOut: std::ptr::null::<AnyObject>()];
                let _: () = msg_send![*w, release];
            }
            return Err(
                "需要辅助功能权限：系统设置 → 隐私与安全性 → 辅助功能 → Voidnix".to_string(),
            );
        }

        // 冻结光标（CGAssociate 解除鼠标硬件与光标位移的关联，事件仍产生但光标不动）
        CGAssociateMouseAndMouseCursorPosition(0);
        // 隐藏光标
        CGDisplayHideCursor(CGMainDisplayID());

        LEFT_DOWN_AT.store(0, Ordering::Relaxed);
        POLL_STOP.store(false, Ordering::Relaxed);
        start_poll_thread();

        let mut s = STATE.lock().map_err(|e| e.to_string())?;
        s.windows = windows;
        s.active = true;
    }
    Ok(())
}

fn disable_clean_mode(app: &AppHandle) -> Result<(), String> {
    {
        let mut s = STATE.lock().map_err(|e| e.to_string())?;
        if !s.active {
            return Ok(());
        }
        POLL_STOP.store(true, Ordering::Relaxed);
        for window in s.windows.drain(..) {
            unsafe {
                let _: () = msg_send![window, orderOut: std::ptr::null::<AnyObject>()];
                let _: () = msg_send![window, release];
            }
        }
        s.active = false;
    }
    stop_keyboard_tap();
    unsafe {
        CGAssociateMouseAndMouseCursorPosition(1);
        CGDisplayShowCursor(CGMainDisplayID());
    }
    crate::runtime::window::hide_main(app);
    Ok(())
}

// ============================================================================
// poll 线程：长按退出 + 180s 兜底 + tap watchdog（三合一）
// ============================================================================

fn start_poll_thread() {
    std::thread::Builder::new()
        .name("clean-mode-poll".into())
        .spawn(|| {
            let start = Instant::now();
            let mut last_watchdog = 0u64;
            // 光标钉点：主屏中心（CGAssociate 兜底——若失效，周期 warp 把光标拉回）
            let display = unsafe { CGMainDisplayID() };
            let center = CGPoint {
                x: unsafe { CGDisplayPixelsWide(display) } as f64 / 2.0,
                y: unsafe { CGDisplayPixelsHigh(display) } as f64 / 2.0,
            };
            loop {
                std::thread::sleep(Duration::from_millis(POLL_MS));
                if POLL_STOP.load(Ordering::Relaxed) {
                    return;
                }
                // 光标钉回中心（CGAssociate 阻止硬件位移，warp 兜底 CGAssociate 失效）
                unsafe { CGWarpMouseCursorPosition(center) };
                // 重新隐藏光标（warp / 窗口前置 / 系统 UI 可能触发指针重新显示）
                unsafe { CGDisplayHideCursor(display) };
                // 长按达标 → 派发到主线程优雅关闭清洁模式（app 继续运行）
                let down_at = LEFT_DOWN_AT.load(Ordering::Relaxed);
                if down_at > 0 && now_ms().saturating_sub(down_at) >= HOLD_MS {
                    eprintln!("[clean-mode] 长按 {HOLD_MS}ms 达标，退出清洁模式");
                    POLL_STOP.store(true, Ordering::Relaxed);
                    if let Some(app) = APP_HANDLE.get() {
                        let app_clone = app.clone();
                        let _ = app.run_on_main_thread(move || {
                            let _ = disable_clean_mode(&app_clone);
                            let _ = app_clone.emit("clean-mode-exit", ());
                        });
                    }
                    return;
                }
                // watchdog：tap 静默禁用则重启（每 WATCHDOG_MS 一次）
                let elapsed_ms = start.elapsed().as_millis() as u64;
                if elapsed_ms.saturating_sub(last_watchdog) >= WATCHDOG_MS {
                    last_watchdog = elapsed_ms;
                    let tap = TAP_REF.load(Ordering::Relaxed);
                    if !tap.is_null() && unsafe { CGEventTapIsEnabled(tap) } == 0 {
                        eprintln!("[clean-mode] watchdog: tap 被禁用，重启");
                        unsafe { CGEventTapEnable(tap, 1) };
                    }
                }
            }
        })
        .expect("spawn clean-mode-poll");
}

// ============================================================================
// Tauri 命令
// ============================================================================

#[tauri::command]
pub async fn set_clean_mode_enabled(app: AppHandle, enabled: bool) -> Result<bool, String> {
    // 退出靠长按（poll 线程 → disable_clean_mode），前端只负责开启
    if !enabled {
        let s = STATE.lock().map_err(|e| e.to_string())?;
        return Ok(s.active);
    }
    {
        let s = STATE.lock().map_err(|e| e.to_string())?;
        if s.active {
            return Ok(true);
        }
    }

    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
    app.run_on_main_thread(move || {
        let _ = tx.send(enable_clean_mode());
    })
    .map_err(|e| e.to_string())?;

    rx.recv_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?
        .map(|()| true)
}

#[tauri::command]
pub async fn is_clean_mode_enabled() -> Result<bool, String> {
    let s = STATE.lock().map_err(|e| e.to_string())?;
    Ok(s.active)
}

// ============================================================================
// 插件注册 + Extension trait
// ============================================================================

pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new("clean-mode").build()
}

pub struct CleanModeExtension;

#[async_trait::async_trait]
impl Extension for CleanModeExtension {
    fn id(&self) -> &'static str {
        "clean-mode"
    }

    async fn setup(&self, app: &AppHandle) -> tauri::Result<()> {
        let _ = APP_HANDLE.set(app.clone());
        Ok(())
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
