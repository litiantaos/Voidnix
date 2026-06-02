// Window snap: 鼠标移入屏幕顶部触发区 → 显示布局面板 → 点击 zone 应用布局
//
// 面板设计（浅色不透明，水平四组）：
//   第 1 组：四宫格（1/4 窗口）— 2×2 网格
//   第 2 组：上下分割（半屏）
//   第 3 组：左右分割（半屏）
//   第 4 组：嵌套三层（外层全屏，中层自定义，内层居中）
//
// 策略：
//   - NSEvent global monitor 捕获 mouseMoved / leftMouseDown
//   - 鼠标进入屏幕顶部中心触发区 → CFRunLoopTimer 轮询（跟踪 + 动画）
//   - 面板带 alpha 渐入渐出动画
//   - 点击面板 zone → 查找最前面的窗口 → 应用布局 → 隐藏面板
//   - 鼠标离开面板 + 触发区 → 渐出隐藏
//   - 布局应用复用 window_manager::platform 的 AX 函数

#[cfg(target_os = "macos")]
mod inner {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    use objc2::runtime::AnyObject;
    use objc2::{ClassType, MainThreadOnly};
    use objc2_app_kit::{NSColor, NSEvent, NSWindow, NSWindowStyleMask};
    use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize};

    use crate::extensions::window_manager::platform::{
        ax_copy_attr, ax_value_to_point, ax_value_to_size, compute_target, set_ax_position,
        set_ax_size, AXIsProcessTrusted, AXUIElementCreateApplication, CFRelease, CFRetain,
        do_get_screens, find_topmost_window_pid,
    };
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
    }

    struct SendVoid(*mut std::ffi::c_void);
    unsafe impl Send for SendVoid {}
    unsafe impl Sync for SendVoid {}

    struct SendPtr(*mut AnyObject);
    unsafe impl Send for SendPtr {}
    unsafe impl Sync for SendPtr {}

    // ── 静态状态 ──────────────────────────────────────────────────────────

    struct MonitorHandles(*mut AnyObject, *mut AnyObject, *mut AnyObject);
    unsafe impl Send for MonitorHandles {}
    unsafe impl Sync for MonitorHandles {}

    static MONITOR: Mutex<Option<MonitorHandles>> = Mutex::new(None);
    static PREVIEW_WINDOW: Mutex<Option<SendPtr>> = Mutex::new(None);
    static ENABLED: AtomicBool = AtomicBool::new(false);
    static POLL_TIMER: Mutex<Option<SendVoid>> = Mutex::new(None);

    struct PanelState {
        visible: bool,
        hovered: Option<&'static str>,
        alpha: f64,
        fading_in: bool,
        fading_out: bool,
        custom_width: f64,
        custom_height: f64,
    }

    static PANEL_STATE: Mutex<PanelState> = Mutex::new(PanelState {
        visible: false,
        hovered: None,
        alpha: 0.0,
        fading_in: false,
        fading_out: false,
        custom_width: 800.0,
        custom_height: 600.0,
    });

    // ── 布局定义 ──────────────────────────────────────────────────────────

    struct ZoneDef {
        layout: &'static str,
        rect: (f64, f64, f64, f64),
    }

    struct ZoneGroup {
        zones: &'static [ZoneDef],
    }

    const GROUP_QUARTER: ZoneGroup = ZoneGroup {
        zones: &[
            ZoneDef { layout: "top-left", rect: (0.0, 0.0, 0.46, 0.46) },
            ZoneDef { layout: "top-right", rect: (0.54, 0.0, 0.46, 0.46) },
            ZoneDef { layout: "bottom-left", rect: (0.0, 0.54, 0.46, 0.46) },
            ZoneDef { layout: "bottom-right", rect: (0.54, 0.54, 0.46, 0.46) },
        ],
    };

    const GROUP_VERTICAL: ZoneGroup = ZoneGroup {
        zones: &[
            ZoneDef { layout: "top", rect: (0.0, 0.0, 1.0, 0.46) },
            ZoneDef { layout: "bottom", rect: (0.0, 0.54, 1.0, 0.46) },
        ],
    };

    const GROUP_HORIZONTAL: ZoneGroup = ZoneGroup {
        zones: &[
            ZoneDef { layout: "left", rect: (0.0, 0.0, 0.46, 1.0) },
            ZoneDef { layout: "right", rect: (0.54, 0.0, 0.46, 1.0) },
        ],
    };

    const GROUP_NESTED: ZoneGroup = ZoneGroup {
        zones: &[
            ZoneDef { layout: "fullscreen", rect: (0.0, 0.0, 1.0, 1.0) },
            ZoneDef { layout: "custom", rect: (0.15, 0.15, 0.7, 0.7) },
            ZoneDef { layout: "center", rect: (0.3, 0.3, 0.4, 0.4) },
        ],
    };

    const ZONE_GROUPS: &[ZoneGroup] = &[GROUP_QUARTER, GROUP_VERTICAL, GROUP_HORIZONTAL, GROUP_NESTED];

    const TRIGGER_ZONE_HEIGHT: f64 = 6.0;
    const PANEL_TOP_GAP: f64 = 8.0;
    const ANIM_STEP: f64 = 0.08;

    // ── 面板尺寸 ──────────────────────────────────────────────────────────

    struct PanelMetrics {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        pad: f64,
        group_gap: f64,
        group_size: f64,
    }

    fn screen_top_inset(screen: &ScreenInfo) -> f64 {
        let Some(mtm) = MainThreadMarker::new() else {
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

    fn compute_panel_metrics(screen: &ScreenInfo) -> PanelMetrics {
        let pad = 12.0;
        let group_gap = 10.0;
        let group_size = 56.0;
        let num_groups = 4.0_f64;
        let panel_w = pad * 2.0 + group_size * num_groups + group_gap * (num_groups - 1.0);
        let panel_h = pad * 2.0 + group_size;
        let x = screen.x + (screen.width - panel_w) / 2.0;
        let y = screen.y + screen.height - panel_h - screen_top_inset(screen) - PANEL_TOP_GAP;
        PanelMetrics {
            x,
            y,
            width: panel_w,
            height: panel_h,
            pad,
            group_gap,
            group_size,
        }
    }

    fn group_local_rect(m: &PanelMetrics, idx: usize) -> NSRect {
        let x = m.pad + idx as f64 * (m.group_size + m.group_gap);
        NSRect::new(NSPoint::new(x, m.pad), NSSize::new(m.group_size, m.group_size))
    }

    fn zone_local_rect(m: &PanelMetrics, group_idx: usize, zone: &ZoneDef) -> NSRect {
        let g = group_local_rect(m, group_idx);
        let (zl, zt, zw, zh) = zone.rect;
        let x = g.origin.x + zl * g.size.width;
        let y_from_top = zt * g.size.height;
        let h = zh * g.size.height;
        let y = g.origin.y + g.size.height - y_from_top - h;
        let w = zw * g.size.width;
        NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
    }

    // ── Hit test ──────────────────────────────────────────────────────────

    fn hit_test(mx: f64, my: f64, m: &PanelMetrics) -> Option<&'static str> {
        let local_x = mx - m.x;
        let local_y = my - m.y;

        for (gi, group) in ZONE_GROUPS.iter().enumerate() {
            let g = group_local_rect(m, gi);
            if local_x < g.origin.x
                || local_x > g.origin.x + g.size.width
                || local_y < g.origin.y
                || local_y > g.origin.y + g.size.height
            {
                continue;
            }
            for zone in group.zones.iter().rev() {
                let z = zone_local_rect(m, gi, zone);
                if local_x >= z.origin.x
                    && local_x <= z.origin.x + z.size.width
                    && local_y >= z.origin.y
                    && local_y <= z.origin.y + z.size.height
                {
                    return Some(zone.layout);
                }
            }
        }

        None
    }

    // ── 绘制 ──────────────────────────────────────────────────────────────

    unsafe fn cg_color(r: f64, g: f64, b: f64, a: f64) -> *mut AnyObject {
        let c: *mut AnyObject = objc2::msg_send![
            objc2::class!(NSColor), colorWithCalibratedRed: r, green: g, blue: b, alpha: a
        ];
        objc2::msg_send![c, CGColor]
    }

    unsafe fn add_layer(
        parent: *mut AnyObject,
        frame: NSRect,
        bg: (f64, f64, f64, f64),
        border: Option<((f64, f64, f64, f64), f64)>,
        radius: f64,
    ) {
        let layer: *mut AnyObject = objc2::msg_send![objc2::class!(CALayer), layer];
        if layer.is_null() {
            return;
        }
        let _: () = objc2::msg_send![layer, setFrame: frame];
        let _: () = objc2::msg_send![layer, setBackgroundColor: cg_color(bg.0, bg.1, bg.2, bg.3)];
        if let Some(((r, g, b, a), w)) = border {
            let _: () = objc2::msg_send![layer, setBorderColor: cg_color(r, g, b, a)];
            let _: () = objc2::msg_send![layer, setBorderWidth: w];
        }
        let _: () = objc2::msg_send![layer, setCornerRadius: radius];
        let _: () = objc2::msg_send![parent, addSublayer: layer];
    }

    unsafe fn draw_panel(win_ptr: *mut AnyObject, m: &PanelMetrics, hovered: Option<&'static str>) {
        let win: &NSWindow = &*(win_ptr as *const NSWindow);
        let cv = win.contentView().unwrap();
        let layer: *mut AnyObject = objc2::msg_send![&cv, layer];
        if layer.is_null() {
            return;
        }
        let _: () = objc2::msg_send![layer, setSublayers: std::ptr::null::<AnyObject>()];

        add_layer(
            layer,
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(m.width, m.height)),
            (0.97, 0.97, 0.98, 1.0),
            Some(((0.0, 0.0, 0.0, 0.08), 1.0)),
            12.0,
        );

        for (gi, group) in ZONE_GROUPS.iter().enumerate() {
            let g = group_local_rect(m, gi);
            let is_nested = gi == 3;

            let group_bg = if is_nested && hovered == Some("fullscreen") {
                (0.24, 0.56, 1.0, 0.12)
            } else {
                (0.0, 0.0, 0.0, 0.03)
            };
            add_layer(layer, g, group_bg, None, 6.0);

            for zone in group.zones.iter() {
                let z = zone_local_rect(m, gi, zone);
                let is_zone_hovered = hovered == Some(zone.layout);

                if is_nested {
                    draw_nested_zone(layer, z, zone.layout, is_zone_hovered);
                } else {
                    let zone_bg = if is_zone_hovered {
                        (0.24, 0.56, 1.0, 0.15)
                    } else {
                        (0.0, 0.0, 0.0, 0.07)
                    };
                    let zone_border = if is_zone_hovered {
                        Some(((0.24, 0.56, 1.0, 0.5), 1.0))
                    } else {
                        None
                    };
                    add_layer(layer, z, zone_bg, zone_border, 3.0);
                }
            }
        }
    }

    unsafe fn draw_nested_zone(
        layer: *mut AnyObject,
        frame: NSRect,
        layout: &'static str,
        is_hovered: bool,
    ) {
        match layout {
            "fullscreen" => {}
            "custom" => {
                let bg = if is_hovered {
                    (0.24, 0.56, 1.0, 0.12)
                } else {
                    (0.0, 0.0, 0.0, 0.04)
                };
                let border = if is_hovered {
                    Some(((0.24, 0.56, 1.0, 0.55), 1.0))
                } else {
                    Some(((0.0, 0.0, 0.0, 0.10), 1.0))
                };
                add_layer(layer, frame, bg, border, 3.0);
            }
            "center" => {
                let bg = if is_hovered {
                    (0.24, 0.56, 1.0, 0.30)
                } else {
                    (0.0, 0.0, 0.0, 0.09)
                };
                add_layer(layer, frame, bg, None, 3.0);
            }
            _ => {}
        }
    }

    // ── 面板窗口生命周期 ──────────────────────────────────────────────────

    unsafe fn ensure_panel_window(m: &PanelMetrics) -> *mut AnyObject {
        let mut guard = PREVIEW_WINDOW.lock().unwrap();
        if let Some(ref sp) = *guard {
            return sp.0;
        }
        let mtm = MainThreadMarker::new().unwrap();
        let rect = NSRect::new(NSPoint::new(m.x, m.y), NSSize::new(m.width, m.height));
        let panel = NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            rect,
            NSWindowStyleMask::from_bits_truncate(0),
            objc2_app_kit::NSBackingStoreType::Buffered,
            false,
        );
        panel.setLevel(objc2_app_kit::NSStatusWindowLevel + 1);
        panel.setOpaque(false);
        panel.setBackgroundColor(Some(&NSColor::clearColor()));
        panel.setIgnoresMouseEvents(false);
        panel.setHasShadow(true);
        let _: () = objc2::msg_send![&panel, setCollectionBehavior:
            objc2_app_kit::NSWindowCollectionBehavior::FullScreenAuxiliary
            | objc2_app_kit::NSWindowCollectionBehavior::Transient
            | objc2_app_kit::NSWindowCollectionBehavior::CanJoinAllSpaces];
        if let Some(cv) = panel.contentView() {
            let _: () = objc2::msg_send![&cv, setWantsLayer: true];
        }
        panel.setAlphaValue(0.0);
        panel.orderFrontRegardless();
        let obj: &AnyObject = &*panel;
        CFRetain(obj as *const AnyObject as *mut std::ffi::c_void);
        let ptr = obj as *const AnyObject as *mut AnyObject;
        *guard = Some(SendPtr(ptr));
        ptr
    }

    unsafe fn set_panel_alpha(alpha: f64) {
        let guard = PREVIEW_WINDOW.lock().unwrap();
        if let Some(ref sp) = *guard {
            let win: &NSWindow = &*(sp.0 as *const NSWindow);
            win.setAlphaValue(alpha);
        }
    }

    unsafe fn set_panel_frame_and_draw(m: &PanelMetrics, hovered: Option<&'static str>) {
        let guard = PREVIEW_WINDOW.lock().unwrap();
        if let Some(ref sp) = *guard {
            let win: &NSWindow = &*(sp.0 as *const NSWindow);
            let rect = NSRect::new(NSPoint::new(m.x, m.y), NSSize::new(m.width, m.height));
            win.setFrame_display(rect, true);
            draw_panel(sp.0, m, hovered);
        }
    }

    unsafe fn destroy_panel() {
        let mut guard = PREVIEW_WINDOW.lock().unwrap();
        if let Some(sp) = guard.take() {
            let raw_ptr = sp.0;
            let w: &NSWindow = &*(raw_ptr as *const NSWindow);
            w.orderOut(None);
            CFRelease(raw_ptr as *mut std::ffi::c_void);
        }
    }

    // ── 鼠标位置 ──────────────────────────────────────────────────────────

    fn get_mouse_location() -> (f64, f64) {
        unsafe {
            let loc: NSPoint = objc2::msg_send![NSEvent::class(), mouseLocation];
            (loc.x, loc.y)
        }
    }

    // ── 轮询 timer（动画 + hover 跟踪）───────────────────────────────────

    unsafe extern "C" fn poll_callback(
        _timer: *mut std::ffi::c_void,
        _info: *mut std::ffi::c_void,
    ) {
        if !ENABLED.load(Ordering::SeqCst) {
            stop_poll_timer();
            return;
        }

        let (mx, my) = get_mouse_location();
        let screen = find_screen_for_point(mx, my);
        let metrics = screen.as_ref().map(|s| compute_panel_metrics(s));

        let in_trigger = match &screen {
            Some(sc) => {
                let screen_top = sc.y + sc.height;
                let center_x = sc.x + sc.width / 2.0;
                my >= screen_top - TRIGGER_ZONE_HEIGHT
                    && mx >= center_x - 100.0
                    && mx <= center_x + 100.0
            }
            None => false,
        };

        let in_panel = match (&screen, &metrics) {
            (Some(sc), Some(m)) => {
                let screen_top = sc.y + sc.height;
                mx >= m.x
                    && mx <= m.x + m.width
                    && my >= m.y - 60.0
                    && my <= screen_top
            }
            _ => false,
        };

        let should_be_visible = in_trigger || in_panel;

        let mut state = PANEL_STATE.lock().unwrap();

        if state.fading_out {
            state.alpha = (state.alpha - ANIM_STEP).max(0.0);
            unsafe { set_panel_alpha(state.alpha); }
            if state.alpha <= 0.0 {
                state.fading_out = false;
                state.visible = false;
                state.hovered = None;
                stop_poll_timer();
            }
        } else if should_be_visible {
            let m = metrics.as_ref().unwrap();
            let hovered: Option<&'static str> = if in_panel {
                hit_test(mx, my, m)
            } else {
                None
            };

            if !state.visible && !state.fading_in {
                state.fading_in = true;
                unsafe {
                    ensure_panel_window(m);
                    set_panel_frame_and_draw(m, hovered);
                }
            }

            if state.fading_in {
                state.alpha = (state.alpha + ANIM_STEP).min(1.0);
                unsafe { set_panel_alpha(state.alpha); }
                if state.alpha >= 1.0 {
                    state.fading_in = false;
                    state.visible = true;
                }
            }

            if state.visible || state.fading_in {
                if let Some(m) = &metrics {
                    unsafe { set_panel_frame_and_draw(m, hovered); }
                }
            }

            state.hovered = hovered;
        } else if state.visible || state.fading_in {
            state.fading_out = true;
            state.fading_in = false;
        } else {
            stop_poll_timer();
        }
    }

    fn start_poll_timer() {
        let mut guard = POLL_TIMER.lock().unwrap();
        if guard.is_some() {
            return;
        }
        unsafe {
            let now = CFAbsoluteTimeGetCurrent();
            let timer = CFRunLoopTimerCreate(
                std::ptr::null_mut(),
                now,
                0.016,
                0,
                0,
                poll_callback,
                std::ptr::null_mut(),
            );
            if timer.is_null() {
                return;
            }
            let rl = CFRunLoopGetCurrent();
            CFRunLoopAddTimer(rl, timer, kCFRunLoopCommonModes);
            *guard = Some(SendVoid(timer));
        }
    }

    fn stop_poll_timer() {
        let mut guard = POLL_TIMER.lock().unwrap();
        if let Some(sv) = guard.take() {
            unsafe {
                let rl = CFRunLoopGetCurrent();
                CFRunLoopRemoveTimer(rl, sv.0, kCFRunLoopCommonModes);
                CFRelease(sv.0);
            }
        }
    }

    // ── 鼠标移动 / 点击 ──────────────────────────────────────────────────

    fn on_mouse_moved() {
        if !ENABLED.load(Ordering::SeqCst) {
            return;
        }
        let state = PANEL_STATE.lock().unwrap();
        let timer_running = POLL_TIMER.lock().unwrap().is_some();
        if state.visible || state.fading_in || state.fading_out || timer_running {
            return;
        }
        drop(state);

        let (mx, my) = get_mouse_location();
        let Some(screen) = find_screen_for_point(mx, my) else {
            return;
        };
        let screen_top = screen.y + screen.height;
        let center_x = screen.x + screen.width / 2.0;
        let in_trigger = my >= screen_top - TRIGGER_ZONE_HEIGHT
            && mx >= center_x - 100.0
            && mx <= center_x + 100.0;

        if in_trigger {
            start_poll_timer();
        }
    }

    fn on_panel_click(mx: f64, my: f64) -> bool {
        let mut state = PANEL_STATE.lock().unwrap();
        if !state.visible {
            return false;
        }

        let screen = find_screen_for_point(mx, my);
        let metrics = screen.as_ref().map(|s| compute_panel_metrics(s));
        let in_panel = match (&screen, &metrics) {
            (Some(_), Some(m)) => {
                mx >= m.x && mx <= m.x + m.width && my >= m.y && my <= m.y + m.height
            }
            _ => false,
        };

        if !in_panel {
            return false;
        }

        let hovered = state.hovered;
        let custom_width = state.custom_width;
        let custom_height = state.custom_height;

        let Some(layout) = hovered else {
            return true;
        };

        state.fading_out = true;
        state.hovered = None;

        let screen = screen.unwrap_or_else(|| ScreenInfo {
            x: 0.0,
            y: 0.0,
            width: 1440.0,
            height: 900.0,
            is_main: true,
        });
        drop(state);

        apply_layout_to_frontmost(layout, &screen, custom_width, custom_height);
        true
    }

    fn apply_layout_to_frontmost(layout: &str, screen: &ScreenInfo, custom_width: f64, custom_height: f64) {
        if !unsafe { AXIsProcessTrusted() } {
            return;
        }

        let Some(pid) = find_topmost_window_pid() else {
            return;
        };

        let Some(win_ref) = (unsafe {
            let ax_app = AXUIElementCreateApplication(pid);
            let win = ax_copy_attr(ax_app, "AXMainWindow")
                .or_else(|| ax_copy_attr(ax_app, "AXFocusedWindow"));
            CFRelease(ax_app);
            win
        }) else {
            return;
        };

        if layout == "center" {
            let frame = unsafe {
                let pos_val = ax_copy_attr(win_ref, "AXPosition");
                let pos = pos_val.as_ref().and_then(|v| ax_value_to_point(*v));
                if let Some(v) = pos_val {
                    CFRelease(v);
                }
                let sz_val = ax_copy_attr(win_ref, "AXSize");
                let sz = sz_val.as_ref().and_then(|v| ax_value_to_size(*v));
                if let Some(v) = sz_val {
                    CFRelease(v);
                }
                pos.zip(sz)
            };
            if let Some(((_px, _py), (sw, sh))) = frame {
                let cx = screen.x + (screen.width - sw) / 2.0;
                let cy = screen.y + (screen.height - sh) / 2.0;
                unsafe {
                    set_ax_position(win_ref, cx, cy);
                }
            }
        } else {
            let (px, py, pw, ph) = compute_target(layout, screen, custom_width, custom_height);
            unsafe {
                set_ax_position(win_ref, px, py);
                set_ax_size(win_ref, pw, ph);
            }
        }

        unsafe {
            CFRelease(win_ref);
        }
    }

    // ── Monitor 生命周期 ─────────────────────────────────────────────────

    pub fn start(app: tauri::AppHandle, custom_width: f64, custom_height: f64) {
        {
            let guard = MONITOR.lock().unwrap();
            if guard.is_some() {
                return;
            }
        }
        ENABLED.store(true, Ordering::SeqCst);
        {
            let mut state = PANEL_STATE.lock().unwrap();
            state.custom_width = custom_width;
            state.custom_height = custom_height;
        }

        let moved_block = block2::RcBlock::new(move |_event: *mut AnyObject| {
            on_mouse_moved();
        });

        let local_moved_block = block2::RcBlock::new(move |event: *mut AnyObject| -> *mut AnyObject {
            on_mouse_moved();
            event
        });

        let down_block = block2::RcBlock::new(move |_event: *mut AnyObject| -> *mut AnyObject {
            let loc: NSPoint = unsafe { objc2::msg_send![NSEvent::class(), mouseLocation] };
            if on_panel_click(loc.x, loc.y) {
                std::ptr::null_mut()
            } else {
                _event
            }
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
            let local_click: *mut AnyObject = objc2::msg_send![
                NSEvent::class(),
                addLocalMonitorForEventsMatchingMask: 1u64 << 1,
                handler: &*down_block
            ];

            if !global_moved.is_null() && !local_moved.is_null() && !local_click.is_null() {
                let mut guard = MONITOR.lock().unwrap();
                *guard = Some(MonitorHandles(global_moved, local_moved, local_click));
                std::mem::forget(moved_block);
                std::mem::forget(local_moved_block);
                std::mem::forget(down_block);
            } else {
                if !global_moved.is_null() {
                    let _: () = objc2::msg_send![NSEvent::class(), removeMonitor: global_moved];
                }
                if !local_moved.is_null() {
                    let _: () = objc2::msg_send![NSEvent::class(), removeMonitor: local_moved];
                }
                if !local_click.is_null() {
                    let _: () = objc2::msg_send![NSEvent::class(), removeMonitor: local_click];
                }
            }
        }
        let _ = app;
    }

    pub fn stop() {
        ENABLED.store(false, Ordering::SeqCst);
        stop_poll_timer();

        let mut guard = MONITOR.lock().unwrap();
        if let Some(mh) = guard.take() {
            unsafe {
                let _: () = objc2::msg_send![NSEvent::class(), removeMonitor: mh.0];
                let _: () = objc2::msg_send![mh.0, release];
                let _: () = objc2::msg_send![NSEvent::class(), removeMonitor: mh.1];
                let _: () = objc2::msg_send![mh.1, release];
                let _: () = objc2::msg_send![NSEvent::class(), removeMonitor: mh.2];
                let _: () = objc2::msg_send![mh.2, release];
            }
        }

        unsafe {
            destroy_panel();
        }

        let mut state = PANEL_STATE.lock().unwrap();
        state.visible = false;
        state.hovered = None;
        state.alpha = 0.0;
        state.fading_in = false;
        state.fading_out = false;
    }

    pub fn is_running() -> bool {
        ENABLED.load(Ordering::SeqCst)
    }
}

#[cfg(not(target_os = "macos"))]
mod inner {
    pub fn start(_app: tauri::AppHandle, _cw: f64, _ch: f64) {}
    pub fn stop() {}
    pub fn is_running() -> bool {
        false
    }
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
