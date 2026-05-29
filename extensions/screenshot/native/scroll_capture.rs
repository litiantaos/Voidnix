use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tauri::Emitter;

use super::ffi::{
    voidnix_screenshot_capture_region, voidnix_screenshot_get_mouse_location,
    voidnix_screenshot_install_scroll_mask, voidnix_screenshot_remove_scroll_mask,
    voidnix_screenshot_set_ignores_mouse, voidnix_screenshot_set_sharing,
    voidnix_screenshot_window_number,
};
use super::ffi::decode_image_data;

pub struct ScrollSession {
    pub sel_x: f64,
    pub sel_y: f64,
    pub sel_w: f64,
    pub sel_h: f64,
    #[allow(dead_code)]
    pub scale: f64,
    pub pw: usize,
    pub ph_per_frame: usize,
    pub buf: Vec<u8>,
    pub total_rows: usize,
    pub prev_frame: Vec<u8>,
    pub overlay_window_id: u32,
    pub ns_window_addr: usize,
    pub ignoring_mouse: bool,
    pub emit_seq: u64,
}

impl ScrollSession {
    pub fn row_bytes(&self) -> usize {
        self.pw * 4
    }
}

pub static SESSION: std::sync::Mutex<Option<ScrollSession>> = std::sync::Mutex::new(None);
pub static IS_RUNNING: AtomicBool = AtomicBool::new(false);
pub static LAST_EMIT_NS: AtomicU64 = AtomicU64::new(0);

pub fn cg_image_to_bgra8(cg: *mut std::ffi::c_void) -> Option<(usize, usize, Vec<u8>)> {
    if cg.is_null() {
        return None;
    }
    unsafe {
        extern "C" {
            fn CGImageGetWidth(image: *mut std::ffi::c_void) -> usize;
            fn CGImageGetHeight(image: *mut std::ffi::c_void) -> usize;
            fn CGColorSpaceCreateDeviceRGB() -> *mut std::ffi::c_void;
            fn CGColorSpaceRelease(cs: *mut std::ffi::c_void);
            fn CGBitmapContextCreate(
                data: *mut std::ffi::c_void,
                width: usize,
                height: usize,
                bits_per_component: usize,
                bytes_per_row: usize,
                space: *mut std::ffi::c_void,
                bitmap_info: u32,
            ) -> *mut std::ffi::c_void;
            fn CGContextDrawImage(
                ctx: *mut std::ffi::c_void,
                rect: core_graphics::geometry::CGRect,
                image: *mut std::ffi::c_void,
            );
            fn CGContextRelease(ctx: *mut std::ffi::c_void);
        }
        let w = CGImageGetWidth(cg);
        let h = CGImageGetHeight(cg);
        if w == 0 || h == 0 {
            return None;
        }
        let row_bytes = w * 4;
        let mut buf = vec![0u8; row_bytes * h];
        let cs = CGColorSpaceCreateDeviceRGB();
        let bitmap_info: u32 = 2 | (2 << 12);
        let ctx = CGBitmapContextCreate(
            buf.as_mut_ptr() as *mut std::ffi::c_void,
            w,
            h,
            8,
            row_bytes,
            cs,
            bitmap_info,
        );
        if ctx.is_null() {
            CGColorSpaceRelease(cs);
            return None;
        }
        let rect = core_graphics::geometry::CGRect {
            origin: core_graphics::geometry::CGPoint { x: 0.0, y: 0.0 },
            size: core_graphics::geometry::CGSize {
                width: w as f64,
                height: h as f64,
            },
        };
        CGContextDrawImage(ctx, rect, cg);
        CGContextRelease(ctx);
        CGColorSpaceRelease(cs);
        Some((w, h, buf))
    }
}

pub fn capture_below_overlay(
    sel_x: f64,
    sel_y: f64,
    sel_w: f64,
    sel_h: f64,
    _overlay_window_id: u32,
) -> Option<(usize, usize, Vec<u8>)> {
    unsafe {
        extern "C" {
            fn CGImageRelease(image: *mut std::ffi::c_void);
        }
        let ns_addr = {
            let g = SESSION.lock().unwrap();
            g.as_ref().map(|s| s.ns_window_addr).unwrap_or(0)
        };
        if ns_addr == 0 {
            return None;
        }

        let mut out_image: *mut std::ffi::c_void = std::ptr::null_mut();
        voidnix_screenshot_capture_region(
            ns_addr as *mut std::ffi::c_void,
            sel_x,
            sel_y,
            sel_w,
            sel_h,
            &mut out_image,
        );
        if out_image.is_null() {
            return None;
        }
        let res = cg_image_to_bgra8(out_image);
        CGImageRelease(out_image);
        res
    }
}

pub fn encode_png(buf: &[u8], width: usize, height: usize) -> Result<Vec<u8>, String> {
    encode_image_with_cg(buf, width, height, "public.png", 1.0)
}

pub fn encode_jpeg(
    buf: &[u8],
    width: usize,
    height: usize,
    quality: f64,
) -> Result<Vec<u8>, String> {
    encode_image_with_cg(buf, width, height, "public.jpeg", quality)
}

fn encode_image_with_cg(
    buf: &[u8],
    width: usize,
    height: usize,
    uti: &str,
    quality: f64,
) -> Result<Vec<u8>, String> {
    if buf.len() != width * height * 4 {
        return Err("buffer 大小不匹配".to_string());
    }
    unsafe {
        extern "C" {
            fn CGColorSpaceCreateDeviceRGB() -> *mut std::ffi::c_void;
            fn CGColorSpaceRelease(cs: *mut std::ffi::c_void);
            fn CGDataProviderCreateWithData(
                info: *mut std::ffi::c_void,
                data: *const u8,
                size: usize,
                release: *mut std::ffi::c_void,
            ) -> *mut std::ffi::c_void;
            fn CGDataProviderRelease(p: *mut std::ffi::c_void);
            fn CGImageCreate(
                width: usize,
                height: usize,
                bits_per_component: usize,
                bits_per_pixel: usize,
                bytes_per_row: usize,
                space: *mut std::ffi::c_void,
                bitmap_info: u32,
                provider: *mut std::ffi::c_void,
                decode: *const f64,
                should_interpolate: bool,
                intent: u32,
            ) -> *mut std::ffi::c_void;
            fn CGImageRelease(image: *mut std::ffi::c_void);
            fn CFDataCreateMutable(
                allocator: *mut std::ffi::c_void,
                capacity: isize,
            ) -> *mut std::ffi::c_void;
            fn CFRelease(p: *mut std::ffi::c_void);
            fn CFStringCreateWithCString(
                allocator: *mut std::ffi::c_void,
                cstr: *const std::os::raw::c_char,
                encoding: u32,
            ) -> *mut std::ffi::c_void;
            fn CGImageDestinationCreateWithData(
                data: *mut std::ffi::c_void,
                uti: *const std::ffi::c_void,
                count: usize,
                options: *mut std::ffi::c_void,
            ) -> *mut std::ffi::c_void;
            fn CGImageDestinationAddImage(
                dst: *mut std::ffi::c_void,
                image: *mut std::ffi::c_void,
                properties: *mut std::ffi::c_void,
            );
            fn CGImageDestinationFinalize(dst: *mut std::ffi::c_void) -> bool;
            fn CFDataGetLength(data: *mut std::ffi::c_void) -> isize;
            fn CFDataGetBytePtr(data: *mut std::ffi::c_void) -> *const u8;
            fn CFNumberCreate(
                allocator: *mut std::ffi::c_void,
                number_type: i32,
                value: *const std::ffi::c_void,
            ) -> *mut std::ffi::c_void;
            fn CFDictionaryCreate(
                allocator: *mut std::ffi::c_void,
                keys: *const *const std::ffi::c_void,
                values: *const *const std::ffi::c_void,
                num_values: isize,
                key_callbacks: *const std::ffi::c_void,
                value_callbacks: *const std::ffi::c_void,
            ) -> *mut std::ffi::c_void;
            static kCFTypeDictionaryKeyCallBacks: std::ffi::c_void;
            static kCFTypeDictionaryValueCallBacks: std::ffi::c_void;
        }

        let cs = CGColorSpaceCreateDeviceRGB();
        let bitmap_info: u32 = 2 | (2 << 12);
        let provider = CGDataProviderCreateWithData(
            std::ptr::null_mut(),
            buf.as_ptr(),
            buf.len(),
            std::ptr::null_mut(),
        );
        if provider.is_null() {
            CGColorSpaceRelease(cs);
            return Err("CGDataProvider 创建失败".to_string());
        }
        let cg_image = CGImageCreate(
            width,
            height,
            8,
            32,
            width * 4,
            cs,
            bitmap_info,
            provider,
            std::ptr::null(),
            false,
            0,
        );
        CGDataProviderRelease(provider);
        CGColorSpaceRelease(cs);
        if cg_image.is_null() {
            return Err("CGImageCreate 失败".to_string());
        }

        let out_data = CFDataCreateMutable(std::ptr::null_mut(), 0);
        if out_data.is_null() {
            CGImageRelease(cg_image);
            return Err("CFDataCreateMutable 失败".to_string());
        }
        let cstr = std::ffi::CString::new(uti).map_err(|e| e.to_string())?;
        let uti_cf = CFStringCreateWithCString(std::ptr::null_mut(), cstr.as_ptr(), 0x08000100);
        if uti_cf.is_null() {
            CFRelease(out_data);
            CGImageRelease(cg_image);
            return Err("CFString 创建失败".to_string());
        }
        let dst = CGImageDestinationCreateWithData(out_data, uti_cf, 1, std::ptr::null_mut());
        CFRelease(uti_cf);
        if dst.is_null() {
            CFRelease(out_data);
            CGImageRelease(cg_image);
            return Err("CGImageDestinationCreateWithData 失败".to_string());
        }

        let mut props_dict: *mut std::ffi::c_void = std::ptr::null_mut();
        if uti == "public.jpeg" {
            let key_cstr =
                std::ffi::CString::new("kCGImageDestinationLossyCompressionQuality").unwrap();
            let key =
                CFStringCreateWithCString(std::ptr::null_mut(), key_cstr.as_ptr(), 0x08000100);
            let q = quality;
            let val = CFNumberCreate(
                std::ptr::null_mut(),
                12,
                &q as *const f64 as *const std::ffi::c_void,
            );
            if !key.is_null() && !val.is_null() {
                let keys: [*const std::ffi::c_void; 1] = [key];
                let vals: [*const std::ffi::c_void; 1] = [val];
                props_dict = CFDictionaryCreate(
                    std::ptr::null_mut(),
                    keys.as_ptr(),
                    vals.as_ptr(),
                    1,
                    &kCFTypeDictionaryKeyCallBacks as *const std::ffi::c_void,
                    &kCFTypeDictionaryValueCallBacks as *const std::ffi::c_void,
                );
            }
            if !key.is_null() {
                CFRelease(key);
            }
            if !val.is_null() {
                CFRelease(val);
            }
        }

        CGImageDestinationAddImage(dst, cg_image, props_dict);
        if !props_dict.is_null() {
            CFRelease(props_dict);
        }
        let ok = CGImageDestinationFinalize(dst);
        CFRelease(dst);
        CGImageRelease(cg_image);

        if !ok {
            CFRelease(out_data);
            return Err("CGImageDestinationFinalize 失败".to_string());
        }

        let len = CFDataGetLength(out_data) as usize;
        let bytes = CFDataGetBytePtr(out_data);
        let result = std::slice::from_raw_parts(bytes, len).to_vec();
        CFRelease(out_data);
        Ok(result)
    }
}

fn row_signatures(frame: &[u8], width: usize, height: usize) -> Vec<u64> {
    let row_bytes = width * 4;
    let mut sigs = Vec::with_capacity(height);
    for r in 0..height {
        let s = r * row_bytes;
        let mut sum: u64 = 0;
        let mut i = s;
        let end = s + row_bytes;
        while i < end {
            sum += frame[i] as u64 + frame[i + 1] as u64 + frame[i + 2] as u64;
            i += 4;
        }
        sigs.push(sum);
    }
    sigs
}

pub fn find_scroll_offset(
    prev_frame: &[u8],
    new_frame: &[u8],
    width: usize,
    height: usize,
) -> (usize, ScrollDir, u64) {
    let prev_sigs = row_signatures(prev_frame, width, height);
    let new_sigs = row_signatures(new_frame, width, height);

    let mut fwd_best_k: usize = 0;
    let mut fwd_best_avg: u64 = u64::MAX;
    for k in 0..height {
        let n = height - k;
        if n < height / 4 {
            break;
        }
        let mut err: u64 = 0;
        for i in 0..n {
            let a = prev_sigs[k + i];
            let b = new_sigs[i];
            err += if a > b { a - b } else { b - a };
        }
        let avg = err / (n as u64);
        if avg < fwd_best_avg {
            fwd_best_avg = avg;
            fwd_best_k = k;
        }
    }

    let mut bwd_best_k: usize = 0;
    let mut bwd_best_avg: u64 = u64::MAX;
    for k in 1..height {
        let n = height - k;
        if n < height / 4 {
            break;
        }
        let mut err: u64 = 0;
        for i in 0..n {
            let a = prev_sigs[i];
            let b = new_sigs[k + i];
            err += if a > b { a - b } else { b - a };
        }
        let avg = err / (n as u64);
        if avg < bwd_best_avg {
            bwd_best_avg = avg;
            bwd_best_k = k;
        }
    }

    if bwd_best_avg < fwd_best_avg.saturating_mul(9) / 10 && bwd_best_k > 0 {
        (bwd_best_k, ScrollDir::Backward, bwd_best_avg)
    } else {
        (fwd_best_k, ScrollDir::Forward, fwd_best_avg)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDir {
    Forward,
    Backward,
}

pub fn append_frame(session: &mut ScrollSession, new_frame: Vec<u8>) -> (usize, bool) {
    let h = session.ph_per_frame;
    let rb = session.row_bytes();
    let frame_bytes = h * rb;

    if session.prev_frame.is_empty() {
        session.buf.extend_from_slice(&new_frame);
        session.total_rows += h;
        session.prev_frame = new_frame;
        return (h, true);
    }

    let (k, dir, confidence_err) =
        find_scroll_offset(&session.prev_frame, &new_frame, session.pw, h);

    let confidence_threshold: u64 = (session.pw as u64) * 32;
    if confidence_err > confidence_threshold {
        return (0, false);
    }

    let mut new_rows: usize = 0;
    let mut changed = false;

    match dir {
        ScrollDir::Backward => {
            let max_trim = session.total_rows.saturating_sub(h);
            if k <= max_trim {
                let new_total = session.total_rows - k;
                session.buf.truncate(new_total * rb);
                session.total_rows = new_total;
            } else {
                session.buf.clear();
                session.buf.extend_from_slice(&new_frame);
                session.total_rows = h;
            }
            changed = true;
        }
        ScrollDir::Forward => {
            if k > 0 {
                let start = (h - k) * rb;
                session.buf.extend_from_slice(&new_frame[start..]);
                session.total_rows += k;
                new_rows = k;
                changed = true;
            }
        }
    }

    if session.total_rows >= h {
        let buf_len = session.buf.len();
        let bottom_start = buf_len - frame_bytes;
        session.buf[bottom_start..buf_len].copy_from_slice(&new_frame);
    }

    session.prev_frame = new_frame;
    (new_rows, changed)
}

pub fn should_emit() -> bool {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let last = LAST_EMIT_NS.load(Ordering::Relaxed);
    if now.saturating_sub(last) < 33_000_000 {
        return false;
    }
    LAST_EMIT_NS.store(now, Ordering::Relaxed);
    true
}

pub fn emit_preview(app: &tauri::AppHandle, session: &mut ScrollSession) {
    if !should_emit() {
        return;
    }
    if session.total_rows == 0 || session.pw == 0 {
        return;
    }
    let jpeg = match encode_jpeg(&session.buf, session.pw, session.total_rows, 0.65) {
        Ok(d) => d,
        Err(_) => return,
    };
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg);
    session.emit_seq += 1;
    let _ = app.emit(
        "screenshot-scroll-frame",
        serde_json::json!({
            "seq": session.emit_seq,
            "width": session.pw,
            "height": session.total_rows,
            "dataUrl": format!("data:image/jpeg;base64,{}", b64),
        }),
    );
}

pub fn capture_loop(app: tauri::AppHandle) {
    const FRAME_INTERVAL_MS: u64 = 12;
    let mut last_ignoring = false;

    while IS_RUNNING.load(Ordering::SeqCst) {
        let (sel_x, sel_y, sel_w, sel_h, overlay_id, cur_ignoring) = {
            let guard = SESSION.lock().unwrap();
            match guard.as_ref() {
                Some(s) => (
                    s.sel_x,
                    s.sel_y,
                    s.sel_w,
                    s.sel_h,
                    s.overlay_window_id,
                    s.ignoring_mouse,
                ),
                None => return,
            }
        };
        if cur_ignoring != last_ignoring {
            let _ = app.emit("screenshot-scroll-passthrough", cur_ignoring);
            last_ignoring = cur_ignoring;
        }

        let frame = capture_below_overlay(sel_x, sel_y, sel_w, sel_h, overlay_id);
        if let Some((fw, fh, fbuf)) = frame {
            let mut guard = SESSION.lock().unwrap();
            if let Some(session) = guard.as_mut() {
                if session.pw == 0 {
                    session.pw = fw;
                    session.ph_per_frame = fh;
                    eprintln!(
                        "[shot-scroll] first frame: {}x{}, buf_len={}, overlay_win_id={}",
                        fw,
                        fh,
                        fbuf.len(),
                        overlay_id
                    );
                }
                if fw == session.pw && fh == session.ph_per_frame {
                    let (added, _changed) = append_frame(session, fbuf);
                    if session.emit_seq == 0 {
                        eprintln!(
                            "[shot-scroll] first append: added={} total_rows={}",
                            added, session.total_rows
                        );
                    }
                    emit_preview(&app, session);
                }
            }
        } else if last_ignoring == cur_ignoring {
            static LOGGED_NONE: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(false);
            if !LOGGED_NONE.swap(true, Ordering::Relaxed) {
                eprintln!(
                    "[shot-scroll] capture_below_overlay returned None (sel={},{},{},{} win={})",
                    sel_x, sel_y, sel_w, sel_h, overlay_id
                );
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(FRAME_INTERVAL_MS));
    }
}

mod mouse_monitor {
    use objc2::runtime::AnyObject;
    use std::sync::Mutex;

    use super::super::ffi::{
        voidnix_screenshot_get_mouse_location, voidnix_screenshot_set_ignores_mouse,
    };
    use super::SESSION;

    struct SendObj(*mut AnyObject);
    unsafe impl Send for SendObj {}
    unsafe impl Sync for SendObj {}

    static GLOBAL_MONITOR: Mutex<SendObj> = Mutex::new(SendObj(std::ptr::null_mut()));
    static LOCAL_MONITOR: Mutex<SendObj> = Mutex::new(SendObj(std::ptr::null_mut()));

    unsafe fn cur_loc() -> (f64, f64) {
        let mut x: f64 = 0.0;
        let mut y: f64 = 0.0;
        voidnix_screenshot_get_mouse_location(&mut x, &mut y, 0.0);
        (x, y)
    }

    unsafe fn check_and_toggle() {
        let (mx, my) = cur_loc();
        let snapshot = {
            let g = SESSION.lock().unwrap();
            match g.as_ref() {
                Some(s) => Some((
                    s.sel_x,
                    s.sel_y,
                    s.sel_w,
                    s.sel_h,
                    s.ns_window_addr,
                    s.ignoring_mouse,
                )),
                None => None,
            }
        };
        let Some((sx, sy, sw, sh, ns_addr, currently_ignoring)) = snapshot else {
            return;
        };
        let in_hole = mx >= sx && mx <= sx + sw && my >= sy && my <= sy + sh;
        if in_hole != currently_ignoring {
            let ptr = ns_addr as *mut std::ffi::c_void;
            voidnix_screenshot_set_ignores_mouse(ptr, if in_hole { 1 } else { 0 });
            let mut g = SESSION.lock().unwrap();
            if let Some(s) = g.as_mut() {
                s.ignoring_mouse = in_hole;
            }
        }
    }

    pub fn start() {
        use objc2::ClassType;
        use objc2_app_kit::NSEvent;
        {
            let g = GLOBAL_MONITOR.lock().unwrap();
            if !g.0.is_null() {
                return;
            }
        }
        let mask: u64 = (1u64 << 5)
            | (1u64 << 6)
            | (1u64 << 7)
            | (1u64 << 27)
            | (1u64 << 22)
            | (1u64 << 1)
            | (1u64 << 2)
            | (1u64 << 3)
            | (1u64 << 4);
        {
            let blk = block2::RcBlock::new(move |_event: *mut AnyObject| {
                unsafe {
                    check_and_toggle();
                }
            });
            unsafe {
                let m: *mut AnyObject = objc2::msg_send![NSEvent::class(), addGlobalMonitorForEventsMatchingMask: mask, handler: &*blk];
                if !m.is_null() {
                    let _: () = objc2::msg_send![m, retain];
                    *GLOBAL_MONITOR.lock().unwrap() = SendObj(m);
                    std::mem::forget(blk);
                }
            }
        }
        {
            let blk = block2::RcBlock::new(move |event: *mut AnyObject| -> *mut AnyObject {
                unsafe {
                    check_and_toggle();
                }
                event
            });
            unsafe {
                let m: *mut AnyObject = objc2::msg_send![NSEvent::class(), addLocalMonitorForEventsMatchingMask: mask, handler: &*blk];
                if !m.is_null() {
                    let _: () = objc2::msg_send![m, retain];
                    *LOCAL_MONITOR.lock().unwrap() = SendObj(m);
                    std::mem::forget(blk);
                }
            }
        }
    }

    pub fn stop() {
        use objc2::ClassType;
        use objc2_app_kit::NSEvent;
        for slot in [&GLOBAL_MONITOR, &LOCAL_MONITOR] {
            let mut g = slot.lock().unwrap();
            if !g.0.is_null() {
                unsafe {
                    let _: () = objc2::msg_send![NSEvent::class(), removeMonitor: g.0];
                    let _: () = objc2::msg_send![g.0, release];
                }
                *g = SendObj(std::ptr::null_mut());
            }
        }
    }
}

pub use mouse_monitor::{start as start_mouse_monitor, stop as stop_mouse_monitor};

// ── Tauri commands ────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn enter_scroll_capture(
    app: tauri::AppHandle,
    sel_x: f64,
    sel_y: f64,
    sel_w: f64,
    sel_h: f64,
    scale: f64,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        if IS_RUNNING.load(Ordering::SeqCst) {
            return Err("滚动截屏已在进行中".to_string());
        }

        let (tx, rx) = std::sync::mpsc::channel::<Result<(u32, usize), String>>();
        let app_c = app.clone();
        app.run_on_main_thread(move || {
            use objc2_app_kit::NSWindow;
            use tauri::Manager;
            let r = (|| -> Result<(u32, usize), String> {
                let window = app_c
                    .get_webview_window("screenshot")
                    .ok_or("找不到截图窗口")?;
                let raw = window
                    .ns_window()
                    .map_err(|e| e.to_string())?
                    .cast::<NSWindow>();
                let ptr = raw.cast::<NSWindow>() as *mut std::ffi::c_void;
                let ns_addr = ptr as usize;
                unsafe {
                    if !voidnix_screenshot_install_scroll_mask(ptr, sel_x, sel_y, sel_w, sel_h) {
                        return Err("装载滚动遮罩失败".to_string());
                    }
                    voidnix_screenshot_set_sharing(ptr, 0);
                    let win_num = voidnix_screenshot_window_number(ptr);
                    if win_num <= 0 {
                        return Err("获取截屏窗口编号失败".to_string());
                    }
                    Ok((win_num as u32, ns_addr))
                }
            })();
            let _ = tx.send(r);
        })
        .map_err(|e| e.to_string())?;
        let (overlay_window_id, ns_window_addr) = rx.recv().map_err(|e| e.to_string())??;

        {
            let mut guard = SESSION.lock().unwrap();
            *guard = Some(ScrollSession {
                sel_x,
                sel_y,
                sel_w,
                sel_h,
                scale,
                pw: 0,
                ph_per_frame: 0,
                buf: Vec::new(),
                total_rows: 0,
                prev_frame: Vec::new(),
                overlay_window_id,
                ns_window_addr,
                ignoring_mouse: false,
                emit_seq: 0,
            });
        }

        let (tx2, rx2) = std::sync::mpsc::channel::<()>();
        app.run_on_main_thread(move || {
            start_mouse_monitor();
            unsafe {
                let mut mx: f64 = 0.0;
                let mut my: f64 = 0.0;
                voidnix_screenshot_get_mouse_location(&mut mx, &mut my, 0.0);
                let g = SESSION.lock().unwrap();
                if let Some(s) = g.as_ref() {
                    let in_hole = mx >= s.sel_x
                        && mx <= s.sel_x + s.sel_w
                        && my >= s.sel_y
                        && my <= s.sel_y + s.sel_h;
                    if in_hole {
                        let ptr = s.ns_window_addr as *mut std::ffi::c_void;
                        voidnix_screenshot_set_ignores_mouse(ptr, 1);
                        drop(g);
                        let mut g2 = SESSION.lock().unwrap();
                        if let Some(s2) = g2.as_mut() {
                            s2.ignoring_mouse = true;
                        }
                    }
                }
            }
            let _ = tx2.send(());
        })
        .map_err(|e| e.to_string())?;
        let _ = rx2.recv();

        IS_RUNNING.store(true, Ordering::SeqCst);

        std::thread::spawn(move || capture_loop(app));
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, sel_x, sel_y, sel_w, sel_h, scale);
        Err("仅支持 macOS".to_string())
    }
}

#[tauri::command]
pub async fn exit_scroll_capture(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        IS_RUNNING.store(false, Ordering::SeqCst);
        std::thread::sleep(std::time::Duration::from_millis(50));
        {
            let mut guard = SESSION.lock().unwrap();
            *guard = None;
        }

        let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let app_c = app.clone();
        app.run_on_main_thread(move || {
            use objc2_app_kit::NSWindow;
            use tauri::Manager;
            let r = (|| -> Result<(), String> {
                stop_mouse_monitor();
                let window = app_c
                    .get_webview_window("screenshot")
                    .ok_or("找不到截图窗口")?;
                let raw = window
                    .ns_window()
                    .map_err(|e| e.to_string())?
                    .cast::<NSWindow>();
                let ptr = raw.cast::<NSWindow>() as *mut std::ffi::c_void;
                unsafe {
                    voidnix_screenshot_remove_scroll_mask(ptr);
                    voidnix_screenshot_set_sharing(ptr, 1);
                }
                Ok(())
            })();
            let _ = tx.send(r);
        })
        .map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())??;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Ok(())
    }
}

#[tauri::command]
pub async fn finish_scroll_capture(app: tauri::AppHandle) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        IS_RUNNING.store(false, Ordering::SeqCst);
        std::thread::sleep(std::time::Duration::from_millis(50));

        let session = {
            let mut guard = SESSION.lock().unwrap();
            guard.take()
        };
        let session = session.ok_or("无滚动截屏会话".to_string())?;
        if session.total_rows == 0 || session.pw == 0 {
            return Err("未捕获到任何内容".to_string());
        }

        let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let app_c = app.clone();
        app.run_on_main_thread(move || {
            use objc2_app_kit::NSWindow;
            use tauri::Manager;
            let r = (|| -> Result<(), String> {
                stop_mouse_monitor();
                let window = app_c
                    .get_webview_window("screenshot")
                    .ok_or("找不到截图窗口")?;
                let raw = window
                    .ns_window()
                    .map_err(|e| e.to_string())?
                    .cast::<NSWindow>();
                let ptr = raw.cast::<NSWindow>() as *mut std::ffi::c_void;
                unsafe {
                    voidnix_screenshot_remove_scroll_mask(ptr);
                    voidnix_screenshot_set_sharing(ptr, 1);
                }
                Ok(())
            })();
            let _ = tx.send(r);
        })
        .map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())??;

        let png = encode_png(&session.buf, session.pw, session.total_rows)?;
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
        Ok(format!("data:image/png;base64,{}", b64))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err("仅支持 macOS".to_string())
    }
}

#[tauri::command]
pub async fn save_scroll_result(
    result_data_url: String,
    path: String,
) -> Result<String, String> {
    let png = decode_image_data(&result_data_url)?;
    let file_path = {
        let p = std::path::Path::new(&path);
        if p.is_dir() {
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            p.join(format!("scroll_screenshot_{}.png", ts))
        } else {
            p.to_path_buf()
        }
    };
    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&file_path, png).map_err(|e| e.to_string())?;
    Ok(file_path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn copy_scroll_result_to_clipboard(result_data_url: String) -> Result<(), String> {
    let png = decode_image_data(&result_data_url)?;
    #[cfg(target_os = "macos")]
    {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let tmp = std::env::temp_dir().join(format!("voidnix_scroll_{}.png", ts));
        std::fs::write(&tmp, &png).map_err(|e| e.to_string())?;
        let script = format!(
            "set f to POSIX file \"{}\"\nset the clipboard to (read f as «class PNGf»)",
            tmp.display()
        );
        let out = Command::new("osascript")
            .args(["-e", &script])
            .output()
            .map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&tmp);
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = png;
        Err("仅支持 macOS".to_string())
    }
}
