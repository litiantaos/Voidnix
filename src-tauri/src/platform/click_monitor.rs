// ============================================================================
// Native click-outside monitor (NSEvent global monitor)
// Works regardless of window focus state — the standard macOS approach
// for overlay/spotlight-style windows.
// ============================================================================

#[cfg(target_os = "macos")]
mod inner {
    use objc2::runtime::AnyObject;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;
    use tauri::{Emitter, Manager};

    /// 配对持有 monitor 对象 + 其 handler block（H5）。
    /// 旧实现 `mem::forget(block)` 让 RcBox 泄漏；改为配对存储，remove 时 drop RcBlock，
    /// 让其 Drop 释放 Rust 侧引用，与 NSEvent 的 retain/release 平衡。
    struct MonitorEntry {
        monitor: *mut AnyObject,
        #[allow(dead_code)]
        block: Option<block2::RcBlock<dyn Fn(*mut AnyObject)>>,
    }
    unsafe impl Send for MonitorEntry {}
    unsafe impl Sync for MonitorEntry {}

    static MONITOR: Mutex<Option<MonitorEntry>> = Mutex::new(None);
    /// 为 true 时跳过 click-outside 发送（原生对话框弹出期间使用）
    static SUPPRESSED: AtomicBool = AtomicBool::new(false);

    pub fn suppress(v: bool) {
        SUPPRESSED.store(v, Ordering::SeqCst);
    }

    pub fn add(app: &tauri::AppHandle) {
        use objc2::ClassType;
        use objc2_app_kit::NSEvent;

        {
            let guard = MONITOR.lock().unwrap();
            if guard.is_some() {
                return;
            }
        }

        let app_handle = app.clone();

        // RcBlock 必须以 trait 对象形式持有，以便存入 MonitorEntry 跨 fn 边界。
        let block: block2::RcBlock<dyn Fn(*mut AnyObject)> =
            block2::RcBlock::new(move |_event: *mut AnyObject| {
                if SUPPRESSED.load(Ordering::SeqCst) {
                    return;
                }
                unsafe {
                    let app = match app_handle.get_webview_window("main") {
                        Some(w) => w,
                        None => return,
                    };
                    if !app.is_visible().unwrap_or(false) {
                        return;
                    }

                    let loc: objc2_foundation::NSPoint =
                        objc2::msg_send![NSEvent::class(), mouseLocation];
                    let click_x = loc.x;
                    let click_y_bottom = loc.y; // NSEvent.mouseLocation 已是屏幕坐标（左下原点）

                    if let (Ok(pos), Ok(size)) = (app.outer_position(), app.outer_size()) {
                        let scale = app.scale_factor().unwrap_or(1.0);
                        let wx = pos.x as f64 / scale;
                        let wy = pos.y as f64 / scale;
                        let ww = size.width as f64 / scale;
                        let wh = size.height as f64 / scale;

                        let main_screen: *mut AnyObject =
                            objc2::msg_send![objc2::class!(NSScreen), mainScreen];
                        if main_screen.is_null() {
                            return;
                        }
                        let frame: objc2_foundation::NSRect = objc2::msg_send![main_screen, frame];
                        let screen_h = frame.size.height;

                        // mouseLocation 是 macOS 屏幕坐标（左下原点），
                        // Tauri outer_position 是物理像素（左上原点），需转换
                        let click_y = screen_h - click_y_bottom;
                        let inside = click_x >= wx
                            && click_x <= wx + ww
                            && click_y >= wy
                            && click_y <= wy + wh;

                        if !inside {
                            let _ = app_handle.emit("click-outside", ());
                        }
                    }
                }
            });

        // SAFETY:
        // - mask = NSEventMaskLeftMouseDown (1 << 1) 是 macOS 标准位掩码
        // - block 经 RcBox 持有，&*block 取引用符合 block2 调用约定
        // - NSEvent addGlobalMonitorForEventsMatchingMask:handler: 保留返回的 monitor 对象
        //   （我们额外 retain 一次以便 remove 时配对 release）
        // - 主线程执行：本函数由 show_main → run_on_main_thread 调度
        unsafe {
            let mask = 1u64 << 1; // NSEventMaskLeftMouseDown
            let monitor: *mut AnyObject = objc2::msg_send![NSEvent::class(), addGlobalMonitorForEventsMatchingMask: mask, handler: &*block];
            if !monitor.is_null() {
                let _: () = objc2::msg_send![monitor, retain];
                let mut guard = MONITOR.lock().unwrap();
                // RcBlock 与 monitor 配对存储：remove 时 monitor release + RcBlock drop 同步释放
                *guard = Some(MonitorEntry {
                    monitor,
                    block: Some(block),
                });
            }
        }
    }

    pub fn remove() {
        use objc2::ClassType;
        use objc2_app_kit::NSEvent;

        let mut guard = MONITOR.lock().unwrap();
        if let Some(entry) = guard.take() {
            // SAFETY: monitor 在 add 中 retain 过一次，此处 removeMonitor + release 配对。
            // entry drop 时 RcBlock drop 释放 Rust 侧引用。
            unsafe {
                let _: () = objc2::msg_send![NSEvent::class(), removeMonitor: entry.monitor];
                let _: () = objc2::msg_send![entry.monitor, release];
            }
            // entry drop → RcBlock drop → Rust 侧引用释放（与 NSEvent 的 retain 平衡）
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod inner {
    pub fn suppress(_v: bool) {}
    pub fn add(_app: &tauri::AppHandle) {}
    pub fn remove() {}
}

// ============================================================================
// Public API
// ============================================================================

/// 添加全局点击外部监视器。
pub fn add(app: &tauri::AppHandle) {
    inner::add(app);
}

/// 移除全局点击外部监视器。
pub fn remove() {
    inner::remove();
}

/// 暂停/恢复 click-outside 检测（原生对话框弹出期间调用）。
pub fn suppress(v: bool) {
    inner::suppress(v);
}
