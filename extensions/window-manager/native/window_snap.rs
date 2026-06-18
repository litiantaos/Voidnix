// Window snap: mouse enters screen top trigger zone → slide Vue snap panel in
// → user clicks zone → invoke set_frontmost_window_layout → apply layout → slide panel out
//
// Rust 只负责：
//   - NSEvent global/local monitor 捕获 mouseMoved
//   - 触发区检测 + 面板窗口滑入/滑出（frame 位移 + ignoresMouseEvents 切换）
//   - 通过 window.eval() 注入自定义尺寸
//
// 面板 UI 完全由 Vue 渲染（SnapPanel.vue），hover 和 click 由 CSS/DOM 处理。

#[cfg(target_os = "macos")]
mod inner {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

    use objc2::runtime::AnyObject;
    use objc2::ClassType;
    use objc2_app_kit::{NSEvent, NSWindow};
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    use tauri::{AppHandle, Manager, WebviewWindow};

    use crate::extensions::window_manager::platform::do_get_screens;
    use crate::extensions::window_manager::ScreenInfo;

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRunLoopTimerCreate(
            alloc: *mut std::ffi::c_void,
            fire_date: f64,
            interval: f64,
            flags: u32,
            order: i64,
            callout: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void),
            context: *mut std::ffi::c_void,
        ) -> *mut std::ffi::c_void;
        fn CFRunLoopAddTimer(
            rl: *mut std::ffi::c_void,
            timer: *mut std::ffi::c_void,
            mode: *mut std::ffi::c_void,
        );
        fn CFRunLoopRemoveTimer(
            rl: *mut std::ffi::c_void,
            timer: *mut std::ffi::c_void,
            mode: *mut std::ffi::c_void,
        );
        fn CFAbsoluteTimeGetCurrent() -> f64;
        fn CFRunLoopGetCurrent() -> *mut std::ffi::c_void;
        static kCFRunLoopCommonModes: *mut std::ffi::c_void;
        fn CFRelease(cf: *mut std::ffi::c_void);
    }

    struct SendTimer(*mut std::ffi::c_void);
    unsafe impl Send for SendTimer {}
    unsafe impl Sync for SendTimer {}

    // ── 静态状态 ──────────────────────────────────────────────────────────

    struct MonitorHandles(*mut AnyObject, *mut AnyObject);
    unsafe impl Send for MonitorHandles {}
    unsafe impl Sync for MonitorHandles {}

    static ENABLED: AtomicBool = AtomicBool::new(false);
    static APP: Mutex<Option<AppHandle>> = Mutex::new(None);
    static MONITOR: Mutex<Option<MonitorHandles>> = Mutex::new(None);
    static HIDE_TIMER: Mutex<Option<SendTimer>> = Mutex::new(None);
    /// 面板显示前的前台应用 PID，隐藏时还原 key window 给该应用。
    static PREV_FRONT_PID: AtomicI32 = AtomicI32::new(0);

    struct SnapState {
        custom_width: f64,
        custom_height: f64,
        visible: bool,
    }

    static STATE: Mutex<SnapState> = Mutex::new(SnapState {
        custom_width: 800.0,
        custom_height: 600.0,
        visible: false,
    });

    // ── 常量 ──────────────────────────────────────────────────────────────

    const TRIGGER_ZONE_HEIGHT: f64 = 6.0;
    const PANEL_TOP_GAP: f64 = 8.0;
    const HIDE_DELAY_SEC: f64 = 0.4;

    fn panel_width() -> f64 { 344.0 }
    fn panel_height() -> f64 { 80.0 }

    // ── 屏幕工具 ──────────────────────────────────────────────────────────

    fn screen_top_inset(screen: &ScreenInfo) -> f64 {
        let Some(mtm) = objc2_foundation::MainThreadMarker::new() else {
            return 0.0;
        };
        let screens = objc2_app_kit::NSScreen::screens(mtm);
        for s in screens.iter() {
            let frame = s.frame();
            if (frame.origin.x - screen.x).abs() < 1.0
                && (frame.origin.y - screen.y).abs() < 1.0
            {
                let visible = s.visibleFrame();
                let frame_top = frame.origin.y + frame.size.height;
                let visible_top = visible.origin.y + visible.size.height;
                return (frame_top - visible_top).max(0.0);
            }
        }
        0.0
    }

    fn find_screen_for_point(mx: f64, my: f64) -> Option<ScreenInfo> {
        let screens = do_get_screens();
        screens
            .iter()
            .find(|s| mx >= s.x && mx <= s.x + s.width && my >= s.y && my <= s.y + s.height)
            .cloned()
    }

    fn compute_panel_rect(screen: &ScreenInfo) -> NSRect {
        let w = panel_width();
        let h = panel_height();
        let x = screen.x + (screen.width - w) / 2.0;
        let y = screen.y + screen.height - h - screen_top_inset(screen) - PANEL_TOP_GAP;
        NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
    }

    fn is_in_trigger_zone(mx: f64, my: f64, screen: &ScreenInfo) -> bool {
        let screen_top = screen.y + screen.height;
        let center_x = screen.x + screen.width / 2.0;
        my >= screen_top - TRIGGER_ZONE_HEIGHT
            && mx >= center_x - 100.0
            && mx <= center_x + 100.0
    }

    fn is_in_panel_area(mx: f64, my: f64, screen: &ScreenInfo) -> bool {
        let rect = compute_panel_rect(screen);
        mx >= rect.origin.x
            && mx <= rect.origin.x + rect.size.width
            && my >= rect.origin.y - 60.0
            && my <= rect.origin.y + rect.size.height
    }

    // ── 鼠标位置 ──────────────────────────────────────────────────────────

    fn get_mouse_location() -> (f64, f64) {
        unsafe {
            let loc: NSPoint = objc2::msg_send![NSEvent::class(), mouseLocation];
            (loc.x, loc.y)
        }
    }

    // ── 面板 show/hide ────────────────────────────────────────────────────

    fn get_snap_window(app: &AppHandle) -> Option<WebviewWindow> {
        app.get_webview_window("snap-panel")
    }

    fn show_panel(app: &AppHandle, screen: &ScreenInfo) {
        let Some(window) = get_snap_window(app) else { return };
        let raw = match window.ns_window() {
            Ok(r) => r,
            Err(_) => return,
        };

        let state = STATE.lock().unwrap();
        let cw = state.custom_width;
        let ch = state.custom_height;
        drop(state);

        // 记录原前台应用 PID,layout 命令需要据此定位要操作的窗口。
        // SnapPanel 不调 makeKeyWindow(只接收鼠标 hover/click),也不 activate
        // NSApp —— 原前台 app 全程是 frontmost + key,菜单栏 / 输入焦点不动。
        let pid = crate::platform::focus::current_frontmost_pid().unwrap_or(0);
        PREV_FRONT_PID.store(pid, Ordering::SeqCst);

        let target = compute_panel_rect(screen);
        unsafe {
            let ns_window = raw.cast::<NSWindow>().as_ref().unwrap();
            ns_window.setFrame_display(target, true);
            ns_window.setAlphaValue(0.0);
            ns_window.setIgnoresMouseEvents(false);
        }

        let _ = window.eval(format!(
            "window.__snapPanelData={{w:{},h:{}}};window.dispatchEvent(new CustomEvent('__snap_panel_show'));",
            cw as i32, ch as i32
        ));

        let mut state = STATE.lock().unwrap();
        state.visible = true;
    }

    fn hide_panel_impl(app: &AppHandle) {
        cancel_hide_timer();

        let Some(window) = get_snap_window(app) else { return };
        let _ = window.eval("window.dispatchEvent(new CustomEvent('__snap_panel_hide'))");

        STATE.lock().unwrap().visible = false;

        // 用户点击 SnapPanel 时,panel 会被 AppKit 自动 makeKey 偷走 system key,
        // 原 app 的 first responder 随之丢失。这里沿用主窗口的恢复策略:
        // deactivate self → activate 原 app,触发系统重新评估 key window,
        // 让输入框光标无缝回到原应用。
        let pid = PREV_FRONT_PID.swap(0, Ordering::SeqCst);
        crate::platform::focus::deactivate_app();
        if pid > 0 {
            crate::platform::focus::activate_app_by_pid(pid);
        }
    }

    // ── Hide timer ────────────────────────────────────────────────────────

    unsafe extern "C" fn hide_timer_callback(
        _timer: *mut std::ffi::c_void,
        _info: *mut std::ffi::c_void,
    ) {
        let app_opt = APP.lock().unwrap().clone();
        let Some(app) = app_opt else { return };
        let app_clone = app.clone();
        let _ = app.run_on_main_thread(move || {
            hide_panel_impl(&app_clone);
        });
    }

    fn schedule_hide_timer() {
        let mut guard = HIDE_TIMER.lock().unwrap();
        if guard.is_some() { return; }
        unsafe {
            let now = CFAbsoluteTimeGetCurrent();
            let timer = CFRunLoopTimerCreate(
                std::ptr::null_mut(),
                now + HIDE_DELAY_SEC,
                0.0,
                0,
                0,
                hide_timer_callback,
                std::ptr::null_mut(),
            );
            if timer.is_null() { return; }
            let rl = CFRunLoopGetCurrent();
            CFRunLoopAddTimer(rl, timer, kCFRunLoopCommonModes);
            *guard = Some(SendTimer(timer));
        }
    }

    fn cancel_hide_timer() {
        let mut guard = HIDE_TIMER.lock().unwrap();
        if let Some(st) = guard.take() {
            unsafe {
                let rl = CFRunLoopGetCurrent();
                CFRunLoopRemoveTimer(rl, st.0, kCFRunLoopCommonModes);
                CFRelease(st.0);
            }
        }
    }

    // ── 鼠标事件 ──────────────────────────────────────────────────────────

    fn forward_mouse_to_snap_panel(mx: f64, my: f64) {
        let app_opt = APP.lock().unwrap().clone();
        let Some(app) = app_opt else { return };
        let Some(window) = get_snap_window(&app) else { return };
        let Ok(raw) = window.ns_window() else { return };
        let local_x;
        let local_y;
        unsafe {
            let ns_window = raw.cast::<NSWindow>().as_ref().unwrap();
            let frame = ns_window.frame();
            local_x = mx - frame.origin.x;
            local_y = (frame.origin.y + frame.size.height) - my;
        }
        let _ = window.eval(format!(
            "window.__snapMouse={{x:{},y:{}}};window.dispatchEvent(new Event('__snap_mouse'));",
            local_x as i32, local_y as i32
        ));
    }

    fn on_mouse_moved() {
        if !ENABLED.load(Ordering::SeqCst) { return; }

        let (mx, my) = get_mouse_location();
        let screen = find_screen_for_point(mx, my);

        let visible = STATE.lock().unwrap().visible;

        if visible {
            let in_area = screen.as_ref().map_or(false, |s| {
                is_in_trigger_zone(mx, my, s) || is_in_panel_area(mx, my, s)
            });
            if in_area {
                cancel_hide_timer();
                forward_mouse_to_snap_panel(mx, my);
            } else {
                schedule_hide_timer();
            }
        } else {
            if let Some(screen) = screen {
                if is_in_trigger_zone(mx, my, &screen) {
                    let app_opt = APP.lock().unwrap().clone();
                    if let Some(app) = app_opt {
                        show_panel(&app, &screen);
                    }
                }
            }
        }
    }

    // ── Monitor 生命周期 ─────────────────────────────────────────────────

    pub fn start(app: AppHandle, custom_width: f64, custom_height: f64) {
        // 始终同步最新尺寸,避免设置变更后旧值被锁死(monitor 已存在则只更新尺寸)
        {
            let mut state = STATE.lock().unwrap();
            state.custom_width = custom_width;
            state.custom_height = custom_height;
        }
        {
            let guard = MONITOR.lock().unwrap();
            if guard.is_some() { return; }
        }
        ENABLED.store(true, Ordering::SeqCst);
        *APP.lock().unwrap() = Some(app);

        let moved_block = block2::RcBlock::new(move |_event: *mut AnyObject| {
            on_mouse_moved();
        });

        let local_moved_block = block2::RcBlock::new(move |event: *mut AnyObject| -> *mut AnyObject {
            on_mouse_moved();
            event
        });

        unsafe {
            let global_moved: *mut AnyObject = objc2::msg_send![
                NSEvent::class(),
                addGlobalMonitorForEventsMatchingMask: 1u64 << 5,
                handler: &*moved_block
            ];
            let local_moved: *mut AnyObject = objc2::msg_send![
                NSEvent::class(),
                addLocalMonitorForEventsMatchingMask: 1u64 << 5,
                handler: &*local_moved_block
            ];

            if !global_moved.is_null() && !local_moved.is_null() {
                let mut guard = MONITOR.lock().unwrap();
                *guard = Some(MonitorHandles(global_moved, local_moved));
                std::mem::forget(moved_block);
                std::mem::forget(local_moved_block);
            } else {
                if !global_moved.is_null() {
                    let _: () = objc2::msg_send![NSEvent::class(), removeMonitor: global_moved];
                }
                if !local_moved.is_null() {
                    let _: () = objc2::msg_send![NSEvent::class(), removeMonitor: local_moved];
                }
            }
        }
    }

    pub fn stop() {
        ENABLED.store(false, Ordering::SeqCst);
        cancel_hide_timer();

        let mut guard = MONITOR.lock().unwrap();
        if let Some(mh) = guard.take() {
            unsafe {
                let _: () = objc2::msg_send![NSEvent::class(), removeMonitor: mh.0];
                let _: () = objc2::msg_send![mh.0, release];
                let _: () = objc2::msg_send![NSEvent::class(), removeMonitor: mh.1];
                let _: () = objc2::msg_send![mh.1, release];
            }
        }

        let app_opt = APP.lock().unwrap().take();
        if let Some(app) = app_opt {
            hide_panel_impl(&app);
        }

        let mut state = STATE.lock().unwrap();
        state.visible = false;
    }

    pub fn is_running() -> bool {
        ENABLED.load(Ordering::SeqCst)
    }

    pub fn hide_panel(app: &AppHandle) {
        if STATE.lock().unwrap().visible {
            hide_panel_impl(app);
        }
    }

    /// 返回面板显示前记录的前台应用 PID（供 layout 命令定位目标窗口）。
    pub fn snap_prev_pid() -> i32 {
        PREV_FRONT_PID.load(Ordering::SeqCst)
    }

    /// 取出并清零暂存的 PID（用于隐藏路径还原焦点，避免重复 activate）。
    pub fn take_snap_prev_pid() -> i32 {
        PREV_FRONT_PID.swap(0, Ordering::SeqCst)
    }
}

#[cfg(not(target_os = "macos"))]
mod inner {
    use tauri::AppHandle;
    pub fn start(_app: AppHandle, _cw: f64, _ch: f64) {}
    pub fn stop() {}
    pub fn is_running() -> bool { false }
    pub fn hide_panel(_app: &AppHandle) {}
    pub fn snap_prev_pid() -> i32 { 0 }
    pub fn take_snap_prev_pid() -> i32 { 0 }
}

pub fn start_drag_monitor(app: tauri::AppHandle, custom_width: f64, custom_height: f64) {
    inner::start(app, custom_width, custom_height);
}

pub fn stop_drag_monitor() {
    inner::stop();
}

pub fn is_drag_monitor_running() -> bool {
    inner::is_running()
}

pub fn hide_panel(app: &tauri::AppHandle) {
    inner::hide_panel(app);
}

pub fn snap_prev_pid() -> i32 {
    inner::snap_prev_pid()
}

pub fn take_snap_prev_pid() -> i32 {
    inner::take_snap_prev_pid()
}
