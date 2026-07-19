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
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

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

    /// 配对持有 monitor 对象 + 其 handler block（H5，同 click_monitor.rs）。
    /// 旧实现 `mem::forget(block)` 让 RcBlock 泄漏且 monitor 未 retain 导致悬垂指针；
    /// 改为配对存储 + 显式 retain，remove 时 drop RcBlock + release monitor。
    struct MonitorHandles {
        global_monitor: *mut AnyObject,
        #[allow(dead_code)]
        global_block: block2::RcBlock<dyn Fn(*mut AnyObject)>,
        local_monitor: *mut AnyObject,
        #[allow(dead_code)]
        local_block: block2::RcBlock<dyn Fn(*mut AnyObject) -> *mut AnyObject>,
    }
    unsafe impl Send for MonitorHandles {}
    unsafe impl Sync for MonitorHandles {}

    static ENABLED: AtomicBool = AtomicBool::new(false);
    static APP: Mutex<Option<AppHandle>> = Mutex::new(None);
    static MONITOR: Mutex<Option<MonitorHandles>> = Mutex::new(None);
    static HIDE_TIMER: Mutex<Option<SendTimer>> = Mutex::new(None);

    struct SnapState {
        custom_width: f64,
        custom_height: f64,
        visible: bool,
    }

    static STATE: Mutex<SnapState> = Mutex::new(SnapState {
        custom_width: 1200.0,
        custom_height: 800.0,
        visible: false,
    });

    // ── 常量 ──────────────────────────────────────────────────────────────

    const TRIGGER_ZONE_HEIGHT: f64 = 6.0;
    const PANEL_TOP_GAP: f64 = 8.0;
    const HIDE_DELAY_SEC: f64 = 0.4;

    /// 与 SnapPanel.vue 同步：p-3×2 + n×w-14 + (n-1)×gap-3；高 p-3×2 + h-14 = 80。
    /// n = 5（单屏）/ 6（多屏，末组 prev/next display）。
    fn group_count(screen_count: usize) -> usize {
        if screen_count > 1 {
            6
        } else {
            5
        }
    }

    fn panel_width_for(screen_count: usize) -> f64 {
        let n = group_count(screen_count) as f64;
        24.0 + n * 56.0 + (n - 1.0) * 12.0
    }

    fn panel_height() -> f64 {
        80.0
    }

    fn current_screen_count() -> usize {
        do_get_screens().len().max(1)
    }

    /// 面板尺寸（show/hide 动画 target 权威源，按当前屏数动态宽）。
    pub(crate) fn panel_dimensions() -> (f64, f64) {
        (panel_width_for(current_screen_count()), panel_height())
    }

    // ── 屏幕工具 ──────────────────────────────────────────────────────────

    fn screen_top_inset(screen: &ScreenInfo) -> f64 {
        let Some(mtm) = objc2_foundation::MainThreadMarker::new() else {
            return 0.0;
        };
        let screens = objc2_app_kit::NSScreen::screens(mtm);
        for s in screens.iter() {
            let frame = s.frame();
            if (frame.origin.x - screen.x).abs() < 1.0 && (frame.origin.y - screen.y).abs() < 1.0 {
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
        let w = panel_width_for(current_screen_count());
        let h = panel_height();
        let x = screen.x + (screen.width - w) / 2.0;
        let y = screen.y + screen.height - h - screen_top_inset(screen) - PANEL_TOP_GAP;
        NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
    }

    fn is_in_trigger_zone(mx: f64, my: f64, screen: &ScreenInfo) -> bool {
        let screen_top = screen.y + screen.height;
        let center_x = screen.x + screen.width / 2.0;
        my >= screen_top - TRIGGER_ZONE_HEIGHT && mx >= center_x - 100.0 && mx <= center_x + 100.0
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
        // SAFETY: mouseLocation 是 NSEvent 类方法（无参数），返回当前全局鼠标坐标；
        // 在主线程调用（本函数由 main-thread monitor 回调链触发）
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
        let Some(window) = get_snap_window(app) else {
            return;
        };
        let raw = match window.ns_window() {
            Ok(r) => r,
            Err(_) => return,
        };

        let state = STATE.lock().unwrap_or_else(|e| e.into_inner());
        let cw = state.custom_width;
        let ch = state.custom_height;
        drop(state);

        // 记录原前台应用 PID,layout 命令需要据此定位要操作的窗口。
        // SnapPanel 不调 makeKeyWindow(只接收鼠标 hover/click),也不 activate
        // NSApp —— 原前台 app 全程是 frontmost + key,菜单栏 / 输入焦点不动。
        // PID 存入 platform::focus 唯一源。
        crate::platform::focus::capture_frontmost();

        let screen_count = current_screen_count();
        let target = compute_panel_rect(screen);
        // SAFETY: ns_window 经 as_ref().unwrap()（raw 来自 ns_window() Ok 分支，非空）；
        // setFrame_display:setAlphaValue:setIgnoresMouseEvents: 均为 NSWindow 标准方法
        unsafe {
            let ns_window = raw.cast::<NSWindow>().as_ref().unwrap();
            ns_window.setFrame_display(target, true);
            ns_window.setAlphaValue(0.0);
            ns_window.setIgnoresMouseEvents(false);
        }

        let _ = window.eval(format!(
            "window.__snapPanelData={{w:{},h:{},screens:{}}};window.dispatchEvent(new CustomEvent('__snap_panel_show'));",
            cw as i32, ch as i32, screen_count
        ));

        let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
        state.visible = true;
    }

    fn hide_panel_impl(app: &AppHandle) {
        cancel_hide_timer();

        let Some(window) = get_snap_window(app) else {
            return;
        };
        let _ = window.eval("window.dispatchEvent(new CustomEvent('__snap_panel_hide'))");

        STATE.lock().unwrap_or_else(|e| e.into_inner()).visible = false;

        // 焦点归还不在此处：hide 经 Vue → hide_snap_panel 做 alpha 淡出，
        // 动画结束后再 restore_captured，避免中途抢焦点导致离场卡顿。
    }

    // ── Hide timer ────────────────────────────────────────────────────────

    unsafe extern "C" fn hide_timer_callback(
        _timer: *mut std::ffi::c_void,
        _info: *mut std::ffi::c_void,
    ) {
        let app_opt = APP.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let Some(app) = app_opt else { return };
        let app_clone = app.clone();
        let _ = app.run_on_main_thread(move || {
            hide_panel_impl(&app_clone);
        });
    }

    fn schedule_hide_timer() {
        let mut guard = HIDE_TIMER.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_some() {
            return;
        }
        // SAFETY: CFRunLoopTimerCreate/Create 系列为 CoreFoundation C API；
        // callout = hide_timer_callback（unsafe extern "C" fn，签名匹配）；
        // context 传 null。timer null 检查后 CFRunLoopAddTimer 加入当前 runloop
        // （本函数在主线程调用），所有权转入 SendTimer 由 cancel 时 CFRelease
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
            if timer.is_null() {
                return;
            }
            let rl = CFRunLoopGetCurrent();
            CFRunLoopAddTimer(rl, timer, kCFRunLoopCommonModes);
            *guard = Some(SendTimer(timer));
        }
    }

    fn cancel_hide_timer() {
        let mut guard = HIDE_TIMER.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(st) = guard.take() {
            // SAFETY: st.0 是已创建的 CFRunLoopTimer（start 时 null 检查通过），
            // 同一 runloop + common modes 配对 Remove；CFRelease 释放 timer 所有权
            unsafe {
                let rl = CFRunLoopGetCurrent();
                CFRunLoopRemoveTimer(rl, st.0, kCFRunLoopCommonModes);
                CFRelease(st.0);
            }
        }
    }

    // ── 鼠标事件 ──────────────────────────────────────────────────────────

    fn forward_mouse_to_snap_panel(mx: f64, my: f64) {
        let app_opt = APP.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let Some(app) = app_opt else { return };
        let Some(window) = get_snap_window(&app) else {
            return;
        };
        let Ok(raw) = window.ns_window() else { return };
        let local_x;
        let local_y;
        // SAFETY: ns_window 经 as_ref().unwrap()（raw 来自 ns_window() Ok 分支，非空）；
        // frame 为 NSWindow 只读属性，返回栈上 NSRect（copy 语义）
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
        if !ENABLED.load(Ordering::SeqCst) {
            return;
        }

        let (mx, my) = get_mouse_location();
        let screen = find_screen_for_point(mx, my);

        let visible = STATE.lock().unwrap_or_else(|e| e.into_inner()).visible;

        if visible {
            let in_area = screen
                .as_ref()
                .is_some_and(|s| is_in_trigger_zone(mx, my, s) || is_in_panel_area(mx, my, s));
            if in_area {
                cancel_hide_timer();
                forward_mouse_to_snap_panel(mx, my);
            } else {
                schedule_hide_timer();
            }
        } else {
            if let Some(screen) = screen {
                if is_in_trigger_zone(mx, my, &screen) {
                    let app_opt = APP.lock().unwrap_or_else(|e| e.into_inner()).clone();
                    if let Some(app) = app_opt {
                        show_panel(&app, &screen);
                    }
                }
            }
        }
    }

    // ── Monitor 生命周期 ─────────────────────────────────────────────────

    /// 仅更新 STATE 尺寸，不启停 monitor（参数推送型，由 set_snap_size 命令消费）。
    pub fn set_size(custom_width: f64, custom_height: f64) {
        let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
        state.custom_width = custom_width;
        state.custom_height = custom_height;
    }

    pub fn start(app: AppHandle) {
        // 尺寸由 set_size 预先推入 STATE，panel 渲染时直接读取
        {
            let guard = MONITOR.lock().unwrap_or_else(|e| e.into_inner());
            if guard.is_some() {
                return;
            }
        }
        ENABLED.store(true, Ordering::SeqCst);
        *APP.lock().unwrap_or_else(|e| e.into_inner()) = Some(app);

        let moved_block: block2::RcBlock<dyn Fn(*mut AnyObject)> =
            block2::RcBlock::new(move |_event: *mut AnyObject| {
                on_mouse_moved();
            });

        let local_moved_block: block2::RcBlock<dyn Fn(*mut AnyObject) -> *mut AnyObject> =
            block2::RcBlock::new(move |event: *mut AnyObject| -> *mut AnyObject {
                on_mouse_moved();
                event
            });

        // SAFETY: mask = 1u64<<5（NSEventMaskMouseMoved）为标准位掩码；
        // block 经 RcBlock 持有，&*block 取引用符合 block2 调用约定；
        // addGlobalMonitor/addLocalMonitor 返回 autoreleased monitor 对象，
        // 显式 retain 一次（与 stop 的 release 配对），RcBlock 配对存储不 forget
        // （同 click_monitor.rs H5）。
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
                let _: () = objc2::msg_send![global_moved, retain];
                let _: () = objc2::msg_send![local_moved, retain];
                let mut guard = MONITOR.lock().unwrap_or_else(|e| e.into_inner());
                *guard = Some(MonitorHandles {
                    global_monitor: global_moved,
                    global_block: moved_block,
                    local_monitor: local_moved,
                    local_block: local_moved_block,
                });
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

        let mut guard = MONITOR.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(mh) = guard.take() {
            // SAFETY: monitor 在 start 中 retain 过一次，此处 removeMonitor + release 配对。
            // mh drop 时 RcBlock drop 释放 Rust 侧引用（与 NSEvent 的 retain 平衡）。
            unsafe {
                let _: () = objc2::msg_send![NSEvent::class(), removeMonitor: mh.global_monitor];
                let _: () = objc2::msg_send![mh.global_monitor, release];
                let _: () = objc2::msg_send![NSEvent::class(), removeMonitor: mh.local_monitor];
                let _: () = objc2::msg_send![mh.local_monitor, release];
            }
        }

        let app_opt = APP.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(app) = app_opt {
            // 仅当 snap-panel 当前可见时才隐藏 + restore focus。
            // 用户在主窗口设置界面点开关关闭时 panel 并未显示，无条件 hide_panel_impl
            // 会触发 restore_captured() → deactivate self → 主窗口失焦隐藏（回归）。
            hide_panel(&app);
        }

        STATE.lock().unwrap_or_else(|e| e.into_inner()).visible = false;
    }

    pub fn hide_panel(app: &AppHandle) {
        if STATE.lock().unwrap_or_else(|e| e.into_inner()).visible {
            hide_panel_impl(app);
        }
    }

    /// 面板逻辑可见（show 后至 hide 发起前）；供 hide 动画结束时条件 restore。
    pub fn is_panel_visible() -> bool {
        STATE.lock().unwrap_or_else(|e| e.into_inner()).visible
    }
}

#[cfg(not(target_os = "macos"))]
mod inner {
    use tauri::AppHandle;
    pub(crate) fn panel_dimensions() -> (f64, f64) {
        (0.0, 0.0)
    }
    pub fn set_size(_cw: f64, _ch: f64) {}
    pub fn start(_app: AppHandle) {}
    pub fn stop() {}
    pub fn is_running() -> bool {
        false
    }
    pub fn hide_panel(_app: &AppHandle) {}
    pub fn is_panel_visible() -> bool {
        false
    }
}

pub fn panel_dimensions() -> (f64, f64) {
    inner::panel_dimensions()
}

pub fn is_panel_visible() -> bool {
    inner::is_panel_visible()
}

pub fn set_snap_size(custom_width: f64, custom_height: f64) {
    inner::set_size(custom_width, custom_height);
}

pub fn start_drag_monitor(app: tauri::AppHandle) {
    inner::start(app);
}

pub fn stop_drag_monitor() {
    inner::stop();
}

pub fn hide_panel(app: &tauri::AppHandle) {
    inner::hide_panel(app);
}
