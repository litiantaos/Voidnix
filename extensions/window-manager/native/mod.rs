use crate::runtime::registry::Extension;
use serde::{Deserialize, Serialize};

mod window_snap;

/// Window manager 扩展。
pub struct Plugin;

impl Extension for Plugin {
    fn id(&self) -> &'static str {
        "window-manager"
    }
}


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ScreenInfo {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub is_main: bool,
}

#[cfg(target_os = "macos")]
pub mod platform {
    use super::*;
    use core_foundation::array::{CFArray, CFArrayRef};
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
    pub const K_AX_VALUE_CGPOINT: u32 = 1;
    pub const K_AX_VALUE_CGSIZE: u32 = 2;

    type CGWindowListOption = u32;
    type CGWindowID = u32;

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
        fn AXValueCreate(
            the_type: u32,
            value_ptr: *const c_void,
        ) -> *mut c_void;
        fn AXValueGetValue(
            value: *mut c_void,
            the_type: u32,
            value_ptr: *mut c_void,
        ) -> bool;
        pub fn CFRelease(cf: *mut c_void);
        pub fn CFRetain(cf: *mut c_void) -> *mut c_void;
        fn CFStringCreateWithCString(
            alloc: *mut c_void,
            c_str: *const i8,
            encoding: u32,
        ) -> *mut c_void;
        fn CGWindowListCopyWindowInfo(
            option: CGWindowListOption,
            relative_to_window: CGWindowID,
        ) -> CFArrayRef;
    }

    const K_CG_WINDOW_LIST_ON_SCREEN_ONLY: CGWindowListOption = 1 << 0;
    const K_CG_WINDOW_LIST_EXCLUDE_DESKTOP: CGWindowListOption = 1 << 4;
    const K_CF_STRING_ENCODING_UTF8: u32 = 0x08000100;

    pub fn cf_str(s: &str) -> *mut c_void {
        let Ok(c) = std::ffi::CString::new(s) else {
            return std::ptr::null_mut();
        };
        unsafe { CFStringCreateWithCString(std::ptr::null_mut(), c.as_ptr(), K_CF_STRING_ENCODING_UTF8) }
    }

    #[repr(C)]
    struct CGPoint { x: f64, y: f64 }
    #[repr(C)]
    struct CGSize { width: f64, height: f64 }

    pub unsafe fn make_ax_value_point(x: f64, y: f64) -> *mut c_void {
        // AXValue 不是 ObjC 类，必须走 C API AXValueCreate
        let pt = CGPoint { x, y };
        AXValueCreate(K_AX_VALUE_CGPOINT, &pt as *const CGPoint as *const c_void)
    }

    pub unsafe fn make_ax_value_size(w: f64, h: f64) -> *mut c_void {
        let sz = CGSize { width: w, height: h };
        AXValueCreate(K_AX_VALUE_CGSIZE, &sz as *const CGSize as *const c_void)
    }

    pub unsafe fn ax_value_to_point(val: *mut c_void) -> Option<(f64, f64)> {
        let mut pt = CGPoint { x: 0.0, y: 0.0 };
        if AXValueGetValue(val, K_AX_VALUE_CGPOINT, &mut pt as *mut CGPoint as *mut c_void) {
            Some((pt.x, pt.y))
        } else {
            None
        }
    }

    pub unsafe fn ax_value_to_size(val: *mut c_void) -> Option<(f64, f64)> {
        let mut sz = CGSize { width: 0.0, height: 0.0 };
        if AXValueGetValue(val, K_AX_VALUE_CGSIZE, &mut sz as *mut CGSize as *mut c_void) {
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
        if err == AX_ERROR_SUCCESS && !val.is_null() { Some(val) } else { None }
    }

    pub unsafe fn set_ax_position(win_ref: *mut c_void, px: f64, py: f64) {
        let pos_val = make_ax_value_point(px, py);
        if pos_val.is_null() { return; }
        let pos_key = cf_str("AXPosition");
        AXUIElementSetAttributeValue(win_ref, pos_key, pos_val);
        CFRelease(pos_key);
        CFRelease(pos_val);
    }

    pub unsafe fn set_ax_size(win_ref: *mut c_void, pw: f64, ph: f64) {
        let size_val = make_ax_value_size(pw, ph);
        if size_val.is_null() { return; }
        let size_key = cf_str("AXSize");
        AXUIElementSetAttributeValue(win_ref, size_key, size_val);
        CFRelease(size_key);
        CFRelease(size_val);
    }

    pub fn compute_target(layout: &str, s: &ScreenInfo, cw: f64, ch: f64) -> (f64, f64, f64, f64) {
        let x = s.x;
        let y = s.y;
        let w = s.width;
        let h = s.height;
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
            "custom" => {
                let clamped_w = cw.clamp(100.0, w);
                let clamped_h = ch.clamp(100.0, h);
                (x + (w - clamped_w) / 2.0, y + (h - clamped_h) / 2.0, clamped_w, clamped_h)
            }
            _ => {
                let clamped_w = cw.clamp(100.0, w);
                let clamped_h = ch.clamp(100.0, h);
                (x + (w - clamped_w) / 2.0, y + (h - clamped_h) / 2.0, clamped_w, clamped_h)
            }
        }
    }

    pub fn do_get_screens() -> Vec<ScreenInfo> {
        let mtm = match MainThreadMarker::new() {
            Some(m) => m,
            None => return vec![ScreenInfo {
                x: 0.0, y: 0.0, width: 1440.0, height: 900.0, is_main: true,
            }],
        };

        let screens = NSScreen::screens(mtm);
        let main_screen = NSScreen::mainScreen(mtm);
        let main_frame = main_screen.map(|s| s.frame()).unwrap_or_default();

        let mut result = Vec::with_capacity(screens.len());
        for screen in screens.iter() {
            let frame = screen.frame();
            let is_main = frame.origin.x == main_frame.origin.x
                && frame.origin.y == main_frame.origin.y
                && frame.size.width == main_frame.size.width
                && frame.size.height == main_frame.size.height;
            result.push(ScreenInfo {
                x: frame.origin.x,
                y: frame.origin.y,
                width: frame.size.width,
                height: frame.size.height,
                is_main,
            });
        }

        if result.is_empty() {
            result.push(ScreenInfo {
                x: main_frame.origin.x,
                y: main_frame.origin.y,
                width: main_frame.size.width,
                height: main_frame.size.height,
                is_main: true,
            });
        }

        result
    }

    fn cf_lookup_num(dict: &CFDictionary<*const c_void, *const c_void>, key: &CFString) -> Option<CFNumber> {
        let ptr = key.as_concrete_TypeRef() as *const c_void;
        let v = dict.find(ptr)?;
        if v.is_null() { return None; }
        Some(unsafe { CFNumber::wrap_under_get_rule(*v as *mut _) })
    }

    fn cf_lookup_dict<'a>(dict: &'a CFDictionary<*const c_void, *const c_void>, key: &CFString) -> Option<CFDictionary<*const c_void, *const c_void>> {
        let ptr = key.as_concrete_TypeRef() as *const c_void;
        let v = dict.find(ptr)?;
        if v.is_null() { return None; }
        Some(unsafe { CFDictionary::wrap_under_get_rule(*v as *const _) })
    }

    pub fn find_topmost_window_pid() -> Option<i32> {
        let raw = unsafe {
            CGWindowListCopyWindowInfo(
                K_CG_WINDOW_LIST_ON_SCREEN_ONLY | K_CG_WINDOW_LIST_EXCLUDE_DESKTOP,
                0,
            )
        };
        if raw.is_null() { return None; }

        let array: CFArray<CFDictionary<*const c_void, *const c_void>> =
            unsafe { CFArray::wrap_under_create_rule(raw) };

        let self_pid = std::process::id() as i64;
        let key_layer = CFString::from_static_string("kCGWindowLayer");
        let key_pid = CFString::from_static_string("kCGWindowOwnerPID");
        let key_alpha = CFString::from_static_string("kCGWindowAlpha");
        let key_bounds = CFString::from_static_string("kCGWindowBounds");
        let key_w = CFString::from_static_string("Width");
        let key_h = CFString::from_static_string("Height");

        for i in 0..array.len() {
            let Some(dict) = array.get(i) else { continue };

            let layer = cf_lookup_num(&dict, &key_layer)
                .and_then(|n| n.to_i64()).unwrap_or(-1);
            if layer != 0 { continue; }

            let pid = cf_lookup_num(&dict, &key_pid)
                .and_then(|n| n.to_i64()).unwrap_or(0);
            if pid == self_pid || pid == 0 { continue; }

            let alpha = cf_lookup_num(&dict, &key_alpha)
                .and_then(|n| n.to_f64()).unwrap_or(1.0);
            if alpha < 0.05 { continue; }

            if let Some(bd) = cf_lookup_dict(&dict, &key_bounds) {
                let w = cf_lookup_num(&bd, &key_w).and_then(|n| n.to_f64()).unwrap_or(0.0);
                let h = cf_lookup_num(&bd, &key_h).and_then(|n| n.to_f64()).unwrap_or(0.0);
                if w < 40.0 || h < 40.0 { continue; }
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
                let cf_arr: CFArray<*const c_void> =
                    CFArray::wrap_under_get_rule(arr as *mut _);
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
        if !output.status.success() { return None; }
        let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if raw.is_empty() { return None; }
        Some(raw.rsplit('/').next().unwrap_or(&raw).to_string())
    }

    fn applescript_set_window_bounds(
        app_name: &str,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    ) -> Result<(), String> {
        let script = format!(
            "tell application \"System Events\"\n\
             tell process \"{}\"\n\
             set position of window 1 to {{{}, {}}}\n\
             set size of window 1 to {{ {}, {} }}\n\
             end tell\n\
             end tell",
            app_name.replace('\\', "\\\\").replace('"', "\\\""),
            x as i32,
            y as i32,
            w as i32,
            h as i32,
        );
        let output = std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .map_err(|e| format!("osascript 执行失败: {}", e))?;
        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("无法调整窗口: {}", stderr.trim()))
        }
    }

    fn set_layout_on_main_thread(layout: &str, custom_width: f64, custom_height: f64, prev_pid: Option<i32>) -> Result<(), String> {
        // 优先级:显式传入 > snap-panel 自己记录的 > 主窗口路径上记录的
        let snap_pid = super::window_snap::snap_prev_pid();
        let fallback_pid = if snap_pid > 0 {
            snap_pid
        } else {
            crate::platform::focus::captured_pid()
        };
        let prev_pid = prev_pid.filter(|&p| p > 0).unwrap_or(fallback_pid);
        let cg_pid = find_topmost_window_pid();

        let primary_pid = if prev_pid > 0 { prev_pid }
            else { cg_pid.ok_or("无法确定目标窗口")? };

        if unsafe { AXIsProcessTrusted() } {
            let win_ref = unsafe { try_get_window_for_pid(primary_pid) }
                .or_else(|| {
                    let fb = cg_pid.filter(|&p| p != primary_pid)?;
                    unsafe { try_get_window_for_pid(fb) }
                });

            if let Some(win_ref) = win_ref {
                let result = unsafe { apply_ax_layout(win_ref, layout, custom_width, custom_height) };
                unsafe { CFRelease(win_ref) };
                return result;
            }
        }

        let app_name = get_process_name(primary_pid)
            .or_else(|| cg_pid.and_then(get_process_name))
            .ok_or("无法获取前台窗口")?;

        let screens = do_get_screens();
        let screen = screens.iter().find(|s| s.is_main).cloned().unwrap_or(ScreenInfo {
            x: 0.0, y: 0.0, width: 1440.0, height: 900.0, is_main: true,
        });

        if layout == "center" {
            return applescript_center_window(&app_name, &screen);
        }

        let (px, py, pw, ph) = compute_target(layout, &screen, custom_width, custom_height);
        applescript_set_window_bounds(&app_name, px, py, pw, ph)
    }

    fn applescript_center_window(app_name: &str, screen: &ScreenInfo) -> Result<(), String> {
        let escaped = app_name.replace('\\', "\\\\").replace('"', "\\\"");
        let get_size = format!(
            "tell application \"System Events\" to tell process \"{}\" to get size of window 1",
            escaped,
        );
        let output = std::process::Command::new("osascript")
            .arg("-e")
            .arg(&get_size)
            .output()
            .map_err(|e| format!("osascript 执行失败: {}", e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("无法获取窗口尺寸: {}", stderr.trim()));
        }
        let size_raw = String::from_utf8_lossy(&output.stdout);
        let size_str = size_raw.trim();
        let dims: Vec<f64> = size_str
            .trim_start_matches('{')
            .trim_end_matches('}')
            .split(", ")
            .filter_map(|s| s.parse().ok())
            .collect();
        if dims.len() < 2 {
            return Err("无法解析窗口尺寸".to_string());
        }
        let (cw, ch) = (dims[0], dims[1]);
        let px = screen.x + (screen.width - cw) / 2.0;
        let py = screen.y + (screen.height - ch) / 2.0;
        applescript_set_window_bounds(app_name, px, py, cw, ch)
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
            if let Some(v) = pos { CFRelease(v); }
            result
        };

        let screens = do_get_screens();
        let target_screen = if let Some((cx, cy)) = current_pos {
            screens.iter().find(|s| {
                cx >= s.x && cx <= s.x + s.width && cy >= s.y && cy <= s.y + s.height
            }).cloned()
        } else {
            None
        };
        let screen = target_screen.unwrap_or_else(|| {
            screens.iter().find(|s| s.is_main).cloned().unwrap_or(ScreenInfo {
                x: 0.0, y: 0.0, width: 1440.0, height: 900.0, is_main: true,
            })
        });

        if layout == "center" {
            let current_size = {
                let sz = ax_copy_attr(win_ref, "AXSize");
                let result = sz.as_ref().and_then(|v| ax_value_to_size(*v));
                if let Some(v) = sz { CFRelease(v); }
                result
            };
            let (cw, ch) = current_size.unwrap_or((800.0, 600.0));
            let px = screen.x + (screen.width - cw) / 2.0;
            let py = screen.y + (screen.height - ch) / 2.0;
            set_ax_position(win_ref, px, py);
            return Ok(());
        }

        let (px, py, pw, ph) = compute_target(layout, &screen, custom_width, custom_height);
        set_ax_position(win_ref, px, py);
        set_ax_size(win_ref, pw, ph);
        Ok(())
    }

    pub fn do_set_layout(app: &tauri::AppHandle, layout: &str, custom_width: f64, custom_height: f64, prev_pid: Option<i32>) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let layout = layout.to_string();
        let app = app.clone();
        let app_clone = app.clone();
        app.run_on_main_thread(move || {
            let result = set_layout_on_main_thread(&layout, custom_width, custom_height, prev_pid);
            super::window_snap::hide_panel(&app_clone);
            let _ = tx.send(result);
        }).map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())?
    }

    pub fn do_check_accessibility() -> bool {
        unsafe { AXIsProcessTrusted() }
    }

    pub fn do_toggle_drag_snap(app: &tauri::AppHandle, enabled: bool, custom_width: f64, custom_height: f64) {
        if enabled {
            super::window_snap::start_drag_monitor(app.clone(), custom_width, custom_height);
        } else {
            super::window_snap::stop_drag_monitor();
        }
    }

    pub fn do_is_drag_snap_active() -> bool {
        super::window_snap::is_drag_monitor_running()
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::*;
    pub fn do_get_screens() -> Vec<ScreenInfo> { vec![] }
    pub fn do_set_layout(_: &tauri::AppHandle, _: &str, _: f64, _: f64, _: Option<i32>) -> Result<(), String> {
        Err("仅支持 macOS".to_string())
    }
    pub fn do_check_accessibility() -> bool { false }
    pub fn do_toggle_drag_snap(_: &tauri::AppHandle, _: bool, _: f64, _: f64) {}
    pub fn do_is_drag_snap_active() -> bool { false }
}

#[tauri::command]
pub fn get_screen_info() -> Vec<ScreenInfo> {
    platform::do_get_screens()
}

#[tauri::command]
pub async fn set_frontmost_window_layout(
    app: tauri::AppHandle,
    layout: String,
    custom_width: Option<f64>,
    custom_height: Option<f64>,
    prev_pid: Option<i32>,
) -> Result<(), String> {
    platform::do_set_layout(
        &app,
        &layout,
        custom_width.unwrap_or(800.0),
        custom_height.unwrap_or(600.0),
        prev_pid,
    )
}

#[tauri::command]
pub fn check_window_manager_accessibility() -> bool {
    platform::do_check_accessibility()
}

#[tauri::command]
pub async fn toggle_drag_snap(
    app: tauri::AppHandle,
    enabled: bool,
    custom_width: Option<f64>,
    custom_height: Option<f64>,
) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let app_clone = app.clone();
    app.run_on_main_thread(move || {
        platform::do_toggle_drag_snap(
            &app_clone,
            enabled,
            custom_width.unwrap_or(800.0),
            custom_height.unwrap_or(600.0),
        );
        let _ = tx.send(());
    }).map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn is_drag_snap_active() -> bool {
    platform::do_is_drag_snap_active()
}

#[tauri::command]
pub async fn show_snap_panel(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let app_clone = app.clone();
    app.run_on_main_thread(move || {
        if let Some(w) = app_clone.get_webview_window("snap-panel") {
            if let Ok(raw) = w.ns_window() {
                unsafe {
                    let ns = raw.cast::<objc2_app_kit::NSWindow>().as_ref().unwrap();
                    use objc2_foundation::MainThreadMarker;
                    let mtm = MainThreadMarker::new().unwrap();
                    if let Some(screen) = objc2_app_kit::NSScreen::mainScreen(mtm) {
                        ns.setFrame_display(screen.frame(), true);
                    }
                    ns.setAlphaValue(1.0);
                    // panel 已是 NonactivatingPanel：makeKey 只让面板接收事件，
                    // 不会把 Voidnix 拉成前台 app，前台应用焦点保持不变；
                    // 同时避免首次点击被「激活点击」吞掉。
                    let _: () = objc2::msg_send![
                        ns,
                        makeKeyAndOrderFront: std::ptr::null::<objc2::runtime::AnyObject>()
                    ];
                }
            }
        }
        let _ = tx.send(());
    }).map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn hide_snap_panel(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let app_clone = app.clone();
    app.run_on_main_thread(move || {
        if let Some(w) = app_clone.get_webview_window("snap-panel") {
            if let Ok(raw) = w.ns_window() {
                unsafe {
                    let ns = raw.cast::<objc2_app_kit::NSWindow>().as_ref().unwrap();
                    ns.setIgnoresMouseEvents(true);
                    ns.setAlphaValue(0.0);
                }
            }
        }
        // 用户点击触发的 layout 路径:panel 已 makeKey 偷走 system key,
        // 隐藏后需 deactivate + activate 原 app,把 first responder 还回去。
        #[cfg(target_os = "macos")]
        {
            let pid = window_snap::take_snap_prev_pid();
            crate::platform::focus::deactivate_app();
            if pid > 0 {
                crate::platform::focus::activate_app_by_pid(pid);
            }
        }
        let _ = tx.send(());
    }).map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())?;
    Ok(())
}

