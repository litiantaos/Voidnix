// AX 窗口枚举 / 布局 / snap 监测（macOS 实现 + 非 macOS stub）。
// 命令入口在 mod.rs。

#[cfg(target_os = "macos")]
mod imp {
    use super::super::*;
    use core_foundation::array::CFArray;
    use core_foundation::base::TCFType;
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use objc2_app_kit::NSScreen;
    use objc2_foundation::MainThreadMarker;
    use std::ffi::c_void;

    pub type AXUIElementRef = *mut c_void;
    type AXError = i32;
    pub const AX_ERROR_SUCCESS: AXError = 0;
    pub const AX_VALUE_CGPOINT: u32 = 1;
    pub const AX_VALUE_CGSIZE: u32 = 2;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        pub fn AXIsProcessTrusted() -> bool;
        pub fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
        fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: *mut c_void,
            value: *mut *mut c_void,
        ) -> AXError;
        fn AXUIElementSetAttributeValue(
            element: AXUIElementRef,
            attribute: *mut c_void,
            value: *mut c_void,
        ) -> AXError;
        fn AXValueCreate(the_type: u32, value_ptr: *const c_void) -> *mut c_void;
        fn AXValueGetValue(value: *mut c_void, the_type: u32, value_ptr: *mut c_void) -> bool;
        pub fn CFRelease(cf: *mut c_void);
        pub fn CFRetain(cf: *mut c_void) -> *mut c_void;
        fn CFStringCreateWithCString(
            alloc: *mut c_void,
            c_str: *const i8,
            encoding: u32,
        ) -> *mut c_void;
    }

    const CF_STRING_ENCODING_UTF8: u32 = 0x08000100;

    pub fn cf_str(s: &str) -> *mut c_void {
        let Ok(c) = std::ffi::CString::new(s) else {
            return std::ptr::null_mut();
        };
        // SAFETY: CFStringCreateWithCString：c 来自 CString::new（NUL 结尾合法），
        // allocator 传 null（使用默认 kCFAllocatorDefault），UTF8 编码常量正确。
        // 返回 Create 规则所有权（调用方 release，本函数返回裸指针由调用方管理）。
        unsafe {
            CFStringCreateWithCString(std::ptr::null_mut(), c.as_ptr(), CF_STRING_ENCODING_UTF8)
        }
    }

    #[repr(C)]
    struct CGPoint {
        x: f64,
        y: f64,
    }
    #[repr(C)]
    struct CGSize {
        width: f64,
        height: f64,
    }

    pub unsafe fn make_ax_value_point(x: f64, y: f64) -> *mut c_void {
        // AXValue 不是 ObjC 类，必须走 C API AXValueCreate
        let pt = CGPoint { x, y };
        AXValueCreate(AX_VALUE_CGPOINT, &pt as *const CGPoint as *const c_void)
    }

    pub unsafe fn make_ax_value_size(w: f64, h: f64) -> *mut c_void {
        let sz = CGSize {
            width: w,
            height: h,
        };
        AXValueCreate(AX_VALUE_CGSIZE, &sz as *const CGSize as *const c_void)
    }

    pub unsafe fn ax_value_to_point(val: *mut c_void) -> Option<(f64, f64)> {
        let mut pt = CGPoint { x: 0.0, y: 0.0 };
        if AXValueGetValue(
            val,
            AX_VALUE_CGPOINT,
            &mut pt as *mut CGPoint as *mut c_void,
        ) {
            Some((pt.x, pt.y))
        } else {
            None
        }
    }

    pub unsafe fn ax_value_to_size(val: *mut c_void) -> Option<(f64, f64)> {
        let mut sz = CGSize {
            width: 0.0,
            height: 0.0,
        };
        if AXValueGetValue(val, AX_VALUE_CGSIZE, &mut sz as *mut CGSize as *mut c_void) {
            Some((sz.width, sz.height))
        } else {
            None
        }
    }

    pub unsafe fn ax_copy_attr(element: AXUIElementRef, attr: &str) -> Option<*mut c_void> {
        let key = cf_str(attr);
        let mut val: *mut c_void = std::ptr::null_mut();
        let err = AXUIElementCopyAttributeValue(element, key, &mut val);
        CFRelease(key);
        if err == AX_ERROR_SUCCESS && !val.is_null() {
            Some(val)
        } else {
            None
        }
    }

    pub unsafe fn set_ax_position(win_ref: *mut c_void, px: f64, py: f64) {
        let pos_val = make_ax_value_point(px, py);
        if pos_val.is_null() {
            return;
        }
        let pos_key = cf_str("AXPosition");
        AXUIElementSetAttributeValue(win_ref, pos_key, pos_val);
        CFRelease(pos_key);
        CFRelease(pos_val);
    }

    pub unsafe fn set_ax_size(win_ref: *mut c_void, pw: f64, ph: f64) {
        let size_val = make_ax_value_size(pw, ph);
        if size_val.is_null() {
            return;
        }
        let size_key = cf_str("AXSize");
        AXUIElementSetAttributeValue(win_ref, size_key, size_val);
        CFRelease(size_key);
        CFRelease(size_val);
    }

    /// AX 只能分别写 size / position；macOS 会按**当前屏**钳制尺寸。
    /// 下半/左下/右下若先 position 再 size，窗口会以旧高度短暂跨出副屏底边
    ///（竖排副屏时直接压到主屏）。顺序：size → position → size（Rectangle 同款）。
    pub unsafe fn set_ax_frame(win_ref: *mut c_void, px: f64, py: f64, pw: f64, ph: f64) {
        set_ax_size(win_ref, pw, ph);
        set_ax_position(win_ref, px, py);
        set_ax_size(win_ref, pw, ph);
    }

    /// 将目标矩形夹进屏的 layout 区（防跨屏钳制后残留越界）。
    fn clamp_to_layout(px: f64, py: f64, pw: f64, ph: f64, s: &ScreenInfo) -> (f64, f64, f64, f64) {
        let max_w = s.layout_width.max(0.0);
        let max_h = s.layout_height.max(0.0);
        let w = pw.clamp(0.0, max_w);
        let h = ph.clamp(0.0, max_h);
        let mut x = px;
        let mut y = py;
        if x + w > s.layout_x + max_w {
            x = s.layout_x + max_w - w;
        }
        if y + h > s.layout_y + max_h {
            y = s.layout_y + max_h - h;
        }
        if x < s.layout_x {
            x = s.layout_x;
        }
        if y < s.layout_y {
            y = s.layout_y;
        }
        (x, y, w, h)
    }

    /// 自定义尺寸夹进 layout；layout 小于 BOUNDS floor 时 floor 降为 layout 边长，避免 `clamp` min>max panic。
    fn clamp_custom_in_layout(
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        cw: f64,
        ch: f64,
    ) -> (f64, f64, f64, f64) {
        let max_w = WIDTH_BOUNDS.1.min(w).max(0.0);
        let max_h = HEIGHT_BOUNDS.1.min(h).max(0.0);
        let min_w = WIDTH_BOUNDS.0.min(max_w);
        let min_h = HEIGHT_BOUNDS.0.min(max_h);
        let clamped_w = cw.clamp(min_w, max_w);
        let clamped_h = ch.clamp(min_h, max_h);
        (
            x + (w - clamped_w) / 2.0,
            y + (h - clamped_h) / 2.0,
            clamped_w,
            clamped_h,
        )
    }

    /// 在目标屏的 AX visible 区内计算布局矩形（AX 坐标）。
    pub fn compute_target(layout: &str, s: &ScreenInfo, cw: f64, ch: f64) -> (f64, f64, f64, f64) {
        let x = s.layout_x;
        let y = s.layout_y;
        let w = s.layout_width;
        let h = s.layout_height;
        let hw = w / 2.0;
        let hh = h / 2.0;

        match layout {
            "top-left" => (x, y, hw, hh),
            "top" => (x, y, w, hh),
            "top-right" => (x + hw, y, hw, hh),
            "left" => (x, y, hw, h),
            "fullscreen" => (x, y, w, h),
            "right" => (x + hw, y, hw, h),
            "bottom-left" => (x, y + hh, hw, hh),
            "bottom" => (x, y + hh, w, hh),
            "bottom-right" => (x + hw, y + hh, hw, hh),
            "custom" | "center" => clamp_custom_in_layout(x, y, w, h, cw, ch),
            _ => clamp_custom_in_layout(x, y, w, h, cw, ch),
        }
    }

    /// AX 点是否落在屏的全帧内（含菜单栏/Dock 带，便于窗口归属判定）。
    fn ax_point_in_frame(px: f64, py: f64, s: &ScreenInfo) -> bool {
        px >= s.ax_frame_x
            && px < s.ax_frame_x + s.ax_frame_width
            && py >= s.ax_frame_y
            && py < s.ax_frame_y + s.ax_frame_height
    }

    /// 按 AX 点选屏；重叠时优先面积更小者（嵌套/对齐边更稳），再退主屏。
    fn screen_for_ax_point(screens: &[ScreenInfo], px: f64, py: f64) -> ScreenInfo {
        let mut best: Option<&ScreenInfo> = None;
        let mut best_area = f64::MAX;
        for s in screens {
            if ax_point_in_frame(px, py, s) {
                let area = s.ax_frame_width * s.ax_frame_height;
                if area < best_area {
                    best_area = area;
                    best = Some(s);
                }
            }
        }
        best.cloned()
            .or_else(|| screens.iter().find(|s| s.is_main).cloned())
            .unwrap_or_else(ScreenInfo::fallback)
    }

    /// 窗口中心点（AX）选屏；无位置则主屏。
    fn screen_for_window(
        screens: &[ScreenInfo],
        pos: Option<(f64, f64)>,
        size: Option<(f64, f64)>,
    ) -> ScreenInfo {
        match (pos, size) {
            (Some((x, y)), Some((w, h))) => screen_for_ax_point(screens, x + w / 2.0, y + h / 2.0),
            (Some((x, y)), None) => screen_for_ax_point(screens, x, y),
            _ => screens
                .iter()
                .find(|s| s.is_main)
                .cloned()
                .unwrap_or_else(ScreenInfo::fallback),
        }
    }

    /// 跨屏：相对位置比例映射到目标屏 visible 区，尺寸按比例缩放并夹紧。
    fn map_window_to_screen(
        win_x: f64,
        win_y: f64,
        win_w: f64,
        win_h: f64,
        from: &ScreenInfo,
        to: &ScreenInfo,
    ) -> (f64, f64, f64, f64) {
        let fw = from.layout_width.max(1.0);
        let fh = from.layout_height.max(1.0);
        let rel_x = (win_x - from.layout_x) / fw;
        let rel_y = (win_y - from.layout_y) / fh;
        let rel_w = (win_w / fw).clamp(0.05, 1.0);
        let rel_h = (win_h / fh).clamp(0.05, 1.0);

        let tw = to.layout_width;
        let th = to.layout_height;
        let mut nw = (rel_w * tw).clamp(WIDTH_BOUNDS.0.min(tw), tw);
        let mut nh = (rel_h * th).clamp(HEIGHT_BOUNDS.0.min(th), th);
        let mut nx = to.layout_x + rel_x * tw;
        let mut ny = to.layout_y + rel_y * th;

        // 夹进目标 visible 区
        if nx + nw > to.layout_x + tw {
            nx = to.layout_x + tw - nw;
        }
        if ny + nh > to.layout_y + th {
            ny = to.layout_y + th - nh;
        }
        if nx < to.layout_x {
            nx = to.layout_x;
        }
        if ny < to.layout_y {
            ny = to.layout_y;
        }
        if nw > tw {
            nw = tw;
        }
        if nh > th {
            nh = th;
        }
        (nx, ny, nw, nh)
    }

    /// `next-display` / `prev-display`：按 NSScreen 枚举序环移。
    fn adjacent_screen<'a>(
        screens: &'a [ScreenInfo],
        current: &ScreenInfo,
        next: bool,
    ) -> Option<&'a ScreenInfo> {
        if screens.len() < 2 {
            return None;
        }
        let idx = screens.iter().position(|s| {
            (s.ax_frame_x - current.ax_frame_x).abs() < 1.0
                && (s.ax_frame_y - current.ax_frame_y).abs() < 1.0
                && (s.ax_frame_width - current.ax_frame_width).abs() < 1.0
        })?;
        let n = screens.len();
        let target = if next {
            (idx + 1) % n
        } else {
            (idx + n - 1) % n
        };
        Some(&screens[target])
    }

    pub fn do_get_screens() -> Vec<ScreenInfo> {
        let mtm = match MainThreadMarker::new() {
            Some(m) => m,
            None => return vec![ScreenInfo::fallback()],
        };

        let screens = NSScreen::screens(mtm);
        if screens.is_empty() {
            return vec![ScreenInfo::fallback()];
        }

        // Primary = 菜单栏坐标系原点屏（frame 近 (0,0)），勿用 mainScreen（随焦点变）。
        let primary_frame = screens
            .iter()
            .map(|s| s.frame())
            .min_by(|a, b| {
                let da = a.origin.x.hypot(a.origin.y);
                let db = b.origin.x.hypot(b.origin.y);
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or_default();
        let primary_max_y = primary_frame.origin.y + primary_frame.size.height;

        // 采集 frame/visible，判定底 Dock 唯一宿主（inset 最大；并列 primary 优先）
        struct RawScreen {
            frame_x: f64,
            frame_y: f64,
            frame_w: f64,
            frame_h: f64,
            vis_x: f64,
            vis_y: f64,
            vis_w: f64,
            vis_h: f64,
            bottom_inset: f64,
            is_primary: bool,
        }

        let raw: Vec<RawScreen> = screens
            .iter()
            .map(|screen| {
                let frame = screen.frame();
                let visible = screen.visibleFrame();
                let is_primary = (frame.origin.x - primary_frame.origin.x).abs() < 1.0
                    && (frame.origin.y - primary_frame.origin.y).abs() < 1.0
                    && (frame.size.width - primary_frame.size.width).abs() < 1.0
                    && (frame.size.height - primary_frame.size.height).abs() < 1.0;
                RawScreen {
                    frame_x: frame.origin.x,
                    frame_y: frame.origin.y,
                    frame_w: frame.size.width,
                    frame_h: frame.size.height,
                    vis_x: visible.origin.x,
                    vis_y: visible.origin.y,
                    vis_w: visible.size.width,
                    vis_h: visible.size.height,
                    bottom_inset: (visible.origin.y - frame.origin.y).max(0.0),
                    is_primary,
                }
            })
            .collect();

        let dock_owner = raw
            .iter()
            .enumerate()
            .filter(|(_, s)| s.bottom_inset > 1.0)
            .max_by(|(_, a), (_, b)| {
                a.bottom_inset
                    .partial_cmp(&b.bottom_inset)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.is_primary.cmp(&b.is_primary))
            })
            .map(|(i, _)| i);

        let mut result = Vec::with_capacity(raw.len());
        for (i, s) in raw.iter().enumerate() {
            let owns_bottom_dock = dock_owner == Some(i);
            let (cx, cy, cw, ch) = ScreenInfo::layout_cocoa_rect(
                (s.frame_x, s.frame_y, s.frame_w, s.frame_h),
                (s.vis_x, s.vis_y, s.vis_w, s.vis_h),
                owns_bottom_dock,
            );

            let (ax_fx, ax_fy, ax_fw, ax_fh) = ScreenInfo::cocoa_rect_to_ax(
                s.frame_x,
                s.frame_y,
                s.frame_w,
                s.frame_h,
                primary_max_y,
            );
            let (lx, ly, lw, lh) = ScreenInfo::cocoa_rect_to_ax(cx, cy, cw, ch, primary_max_y);

            result.push(ScreenInfo {
                x: s.frame_x,
                y: s.frame_y,
                width: s.frame_w,
                height: s.frame_h,
                // is_main：焦点 mainScreen 语义改为 primary（坐标系锚点屏），布局选屏不依赖它
                is_main: s.is_primary,
                layout_x: lx,
                layout_y: ly,
                layout_width: lw,
                layout_height: lh,
                ax_frame_x: ax_fx,
                ax_frame_y: ax_fy,
                ax_frame_width: ax_fw,
                ax_frame_height: ax_fh,
            });
        }

        if result.is_empty() {
            result.push(ScreenInfo::fallback());
        }

        result
    }

    pub fn find_topmost_window_pid() -> Option<i32> {
        use crate::platform::window_list::dict_lookup;
        let array = crate::platform::window_list::copy_on_screen_windows()?;

        let self_pid = std::process::id() as i64;
        let key_layer = CFString::from_static_string("kCGWindowLayer");
        let key_pid = CFString::from_static_string("kCGWindowOwnerPID");
        let key_alpha = CFString::from_static_string("kCGWindowAlpha");
        let key_bounds = CFString::from_static_string("kCGWindowBounds");
        let key_w = CFString::from_static_string("Width");
        let key_h = CFString::from_static_string("Height");

        for i in 0..array.len() {
            let Some(dict) = array.get(i) else { continue };

            let layer = dict_lookup(&dict, &key_layer)
                .and_then(|v| v.downcast::<CFNumber>())
                .and_then(|n| n.to_i64())
                .unwrap_or(-1);
            if layer != 0 {
                continue;
            }

            let pid = dict_lookup(&dict, &key_pid)
                .and_then(|v| v.downcast::<CFNumber>())
                .and_then(|n| n.to_i64())
                .unwrap_or(0);
            if pid == self_pid || pid == 0 {
                continue;
            }

            let alpha = dict_lookup(&dict, &key_alpha)
                .and_then(|v| v.downcast::<CFNumber>())
                .and_then(|n| n.to_f64())
                .unwrap_or(1.0);
            if alpha < 0.05 {
                continue;
            }

            if let Some(bd) = dict_lookup(&dict, &key_bounds)
                .and_then(|v| v.downcast::<CFDictionary<*const c_void, *const c_void>>())
            {
                let w = dict_lookup(&bd, &key_w)
                    .and_then(|v| v.downcast::<CFNumber>())
                    .and_then(|n| n.to_f64())
                    .unwrap_or(0.0);
                let h = dict_lookup(&bd, &key_h)
                    .and_then(|v| v.downcast::<CFNumber>())
                    .and_then(|n| n.to_f64())
                    .unwrap_or(0.0);
                if w < 40.0 || h < 40.0 {
                    continue;
                }
            } else {
                continue;
            }

            return Some(pid as i32);
        }

        None
    }

    unsafe fn try_get_window_for_pid(pid: i32) -> Option<*mut c_void> {
        let ax_app = AXUIElementCreateApplication(pid);
        let win = ax_copy_attr(ax_app, "AXMainWindow")
            .or_else(|| ax_copy_attr(ax_app, "AXFocusedWindow"))
            .or_else(|| {
                let arr = ax_copy_attr(ax_app, "AXWindows")?;
                let cf_arr: CFArray<*const c_void> = CFArray::wrap_under_get_rule(arr as *mut _);
                let first = cf_arr.get(0).filter(|w| !w.is_null())?;
                let win_ptr = *first as *mut c_void;
                CFRetain(win_ptr);
                CFRelease(arr);
                Some(win_ptr)
            });
        CFRelease(ax_app);
        win
    }

    fn get_process_name(pid: i32) -> Option<String> {
        let output = std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "comm="])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if raw.is_empty() {
            return None;
        }
        Some(raw.rsplit('/').next().unwrap_or(&raw).to_string())
    }

    fn applescript_set_window_bounds(
        app_name: &str,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    ) -> Result<(), String> {
        // 与 set_ax_frame 同序：size → position → size，避免下半区跨屏钳制
        let script = format!(
            "tell application \"System Events\"\n\
             tell process \"{}\"\n\
             set size of window 1 to {{{}, {}}}\n\
             set position of window 1 to {{{}, {}}}\n\
             set size of window 1 to {{{}, {}}}\n\
             end tell\n\
             end tell",
            app_name.replace('\\', "\\\\").replace('"', "\\\""),
            w as i32,
            h as i32,
            x as i32,
            y as i32,
            w as i32,
            h as i32,
        );
        let output = std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .map_err(|e| format!("osascript 执行失败: {e}"))?;
        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("无法调整窗口: {}", stderr.trim()))
        }
    }

    fn set_layout_on_main_thread(
        layout: &str,
        custom_width: f64,
        custom_height: f64,
        prev_pid: Option<i32>,
    ) -> Result<(), String> {
        // 优先级:显式传入 > focus 唯一源记录的（snap-panel show 时 capture）
        let fallback_pid = crate::platform::focus::captured_pid();
        let prev_pid = prev_pid.filter(|&p| p > 0).unwrap_or(fallback_pid);
        let cg_pid = find_topmost_window_pid();

        let primary_pid = if prev_pid > 0 {
            prev_pid
        } else {
            cg_pid.ok_or("无法确定目标窗口")?
        };

        // SAFETY: AXIsProcessTrusted 是 Accessibility C API，无参数，仅查询当前进程可信状态
        if unsafe { AXIsProcessTrusted() } {
            // SAFETY: primary_pid > 0（上方过滤）；try_get_window_for_pid 为 unsafe fn，
            // 内部 AXUIElementCreateApplication/AXUIElementCopyAttributeValue 均 null-safe
            // 且对返回值 CFRelease 配平
            let win_ref = unsafe { try_get_window_for_pid(primary_pid) }.or_else(|| {
                let fb = cg_pid.filter(|&p| p != primary_pid)?;
                // SAFETY: fb > 0（cg_pid 为 i32 pid）；同 try_get_window_for_pid 契约
                unsafe { try_get_window_for_pid(fb) }
            });

            if let Some(win_ref) = win_ref {
                let result =
                    // SAFETY: win_ref 非 null（Some 分支）；apply_ax_layout 为 unsafe fn，
                    // 所有 AX 读写均经 ax_copy_attr/set_ax_* 封装（内部 null 检查 + CFRelease 配平）
                    unsafe { apply_ax_layout(win_ref, layout, custom_width, custom_height) };
                // SAFETY: win_ref 由 AXUIElementCreateApplication + CFRetain 创建（+1 retain），
                // 此处 release 配平，避免泄漏
                unsafe { CFRelease(win_ref) };
                return result;
            }
        }

        let app_name = get_process_name(primary_pid)
            .or_else(|| cg_pid.and_then(get_process_name))
            .ok_or("无法获取前台窗口")?;

        applescript_apply_layout(&app_name, layout, custom_width, custom_height)
    }

    /// 解析 AppleScript `{a, b}` / `a, b` 输出。
    fn parse_applescript_pair(raw: &str) -> Option<(f64, f64)> {
        let dims: Vec<f64> = raw
            .trim()
            .trim_start_matches('{')
            .trim_end_matches('}')
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        if dims.len() >= 2 {
            Some((dims[0], dims[1]))
        } else {
            None
        }
    }

    /// (position, size) 均为 AX 坐标。
    #[allow(clippy::type_complexity)]
    fn applescript_get_window_geom(app_name: &str) -> Result<((f64, f64), (f64, f64)), String> {
        let escaped = app_name.replace('\\', "\\\\").replace('"', "\\\"");
        let script = format!(
            "tell application \"System Events\" to tell process \"{}\"\n\
             set p to position of window 1\n\
             set s to size of window 1\n\
             return (item 1 of p as text) & \",\" & (item 2 of p as text) & \";\" & \
                    (item 1 of s as text) & \",\" & (item 2 of s as text)\n\
             end tell",
            escaped,
        );
        let output = std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .map_err(|e| format!("osascript 执行失败: {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("无法获取窗口几何: {}", stderr.trim()));
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let mut parts = text.trim().split(';');
        let pos = parts
            .next()
            .and_then(parse_applescript_pair)
            .ok_or_else(|| "无法解析窗口位置".to_string())?;
        let size = parts
            .next()
            .and_then(parse_applescript_pair)
            .ok_or_else(|| "无法解析窗口尺寸".to_string())?;
        Ok((pos, size))
    }

    fn applescript_apply_layout(
        app_name: &str,
        layout: &str,
        custom_width: f64,
        custom_height: f64,
    ) -> Result<(), String> {
        let screens = do_get_screens();
        let (pos, size) = applescript_get_window_geom(app_name)?;
        let screen = screen_for_window(&screens, Some(pos), Some(size));

        if layout == "next-display" || layout == "prev-display" {
            let Some(target) = adjacent_screen(&screens, &screen, layout == "next-display") else {
                return Ok(());
            };
            let (px, py, pw, ph) =
                map_window_to_screen(pos.0, pos.1, size.0, size.1, &screen, target);
            let (px, py, pw, ph) = clamp_to_layout(px, py, pw, ph, target);
            return applescript_set_window_bounds(app_name, px, py, pw, ph);
        }

        if layout == "center" {
            // 居中保持原尺寸；勿 clamp（大窗相对 layout 负偏移是正确居中）
            let (cw, ch) = size;
            let px = screen.layout_x + (screen.layout_width - cw) / 2.0;
            let py = screen.layout_y + (screen.layout_height - ch) / 2.0;
            return applescript_set_window_bounds(app_name, px, py, cw, ch);
        }

        let (px, py, pw, ph) = compute_target(layout, &screen, custom_width, custom_height);
        let (px, py, pw, ph) = clamp_to_layout(px, py, pw, ph, &screen);
        applescript_set_window_bounds(app_name, px, py, pw, ph)
    }

    unsafe fn apply_ax_layout(
        win_ref: *mut c_void,
        layout: &str,
        custom_width: f64,
        custom_height: f64,
    ) -> Result<(), String> {
        let current_pos = {
            let pos = ax_copy_attr(win_ref, "AXPosition");
            let result = pos.as_ref().and_then(|v| ax_value_to_point(*v));
            if let Some(v) = pos {
                CFRelease(v);
            }
            result
        };
        let current_size = {
            let sz = ax_copy_attr(win_ref, "AXSize");
            let result = sz.as_ref().and_then(|v| ax_value_to_size(*v));
            if let Some(v) = sz {
                CFRelease(v);
            }
            result
        };

        let screens = do_get_screens();
        let screen = screen_for_window(&screens, current_pos, current_size);

        if layout == "next-display" || layout == "prev-display" {
            let Some(target) = adjacent_screen(&screens, &screen, layout == "next-display") else {
                return Ok(());
            };
            let (wx, wy) = current_pos.unwrap_or((screen.layout_x, screen.layout_y));
            let (ww, wh) =
                current_size.unwrap_or((screen.layout_width * 0.5, screen.layout_height * 0.5));
            let (px, py, pw, ph) = map_window_to_screen(wx, wy, ww, wh, &screen, target);
            let (px, py, pw, ph) = clamp_to_layout(px, py, pw, ph, target);
            set_ax_frame(win_ref, px, py, pw, ph);
            return Ok(());
        }

        if layout == "center" {
            // 居中只改位置、保持原尺寸；勿 clamp（大窗相对 layout 负偏移是正确居中）
            let (cw, ch) = current_size.unwrap_or((1200.0, 800.0));
            let px = screen.layout_x + (screen.layout_width - cw) / 2.0;
            let py = screen.layout_y + (screen.layout_height - ch) / 2.0;
            set_ax_position(win_ref, px, py);
            return Ok(());
        }

        let (px, py, pw, ph) = compute_target(layout, &screen, custom_width, custom_height);
        let (px, py, pw, ph) = clamp_to_layout(px, py, pw, ph, &screen);
        set_ax_frame(win_ref, px, py, pw, ph);
        Ok(())
    }

    pub fn do_set_layout(
        app: &tauri::AppHandle,
        layout: &str,
        custom_width: f64,
        custom_height: f64,
        prev_pid: Option<i32>,
    ) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let layout = layout.to_string();
        let app = app.clone();
        let app_clone = app.clone();
        app.run_on_main_thread(move || {
            let result = set_layout_on_main_thread(&layout, custom_width, custom_height, prev_pid);
            super::super::window_snap::hide_panel(&app_clone);
            let _ = tx.send(result);
        })
        .map_err(|e| e.to_string())?;
        rx.recv_timeout(std::time::Duration::from_secs(10))
            .map_err(|e| e.to_string())?
    }

    pub fn do_set_window_manager_enabled(app: &tauri::AppHandle, enabled: bool) {
        if enabled {
            super::super::window_snap::start_drag_monitor(app.clone());
        } else {
            super::super::window_snap::stop_drag_monitor();
        }
    }

    pub fn do_set_snap_size(width: f64, height: f64) {
        super::super::window_snap::set_snap_size(width, height);
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::super::*;
    pub fn do_get_screens() -> Vec<ScreenInfo> {
        vec![]
    }
    pub fn do_set_layout(
        _: &tauri::AppHandle,
        _: &str,
        _: f64,
        _: f64,
        _: Option<i32>,
    ) -> Result<(), String> {
        Err("仅支持 macOS".to_string())
    }
    pub fn do_set_window_manager_enabled(_: &tauri::AppHandle, _: bool) {}
    pub fn do_set_snap_size(_: f64, _: f64) {}
}

pub use imp::*;
