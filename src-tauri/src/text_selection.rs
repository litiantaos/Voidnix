//! macOS 选中文本提取
//!
//! Layer 1: AXUIElementCreateSystemWide → kAXFocusedUIElement → kAXSelectedText
//! Layer 2: CGEventPostToPid(Cmd+C) + NSPasteboard changeCount 轮询
//!
//! Layer 2 必须在后台线程执行（sleep 轮询），且 CGEventPostToPid 需要在
//! Voidnix 抢焦点之前调用，所以调用方需在主线程先 snapshot + inject，
//! 再切后台线程轮询。

use std::ffi::{c_void, CStr, CString};
use std::time::{Duration, Instant};

// ============================================================================
// Layer 1: AX system-wide（不依赖 PID）
// ============================================================================

#[cfg(target_os = "macos")]
mod ax {
    use super::*;

    type AXUIElementRef = *mut c_void;
    type AXError = i32;
    const K_AX_ERROR_SUCCESS: AXError = 0;
    const K_CF_STRING_ENCODING_UTF8: u32 = 0x08000100;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXUIElementCreateSystemWide() -> AXUIElementRef;
        fn AXUIElementSetMessagingTimeout(element: AXUIElementRef, timeout_in_seconds: f32) -> AXError;
        fn AXUIElementCopyAttributeValue(element: AXUIElementRef, attribute: *mut c_void, value: *mut *mut c_void) -> AXError;
        fn AXIsProcessTrustedWithOptions(options: *mut c_void) -> bool;
        fn CFGetTypeID(cf: *mut c_void) -> usize;
        fn CFStringGetTypeID() -> usize;
        fn CFRelease(cf: *mut c_void);
        fn CFStringGetLength(theString: *mut c_void) -> isize;
        fn CFStringGetMaximumSizeForEncoding(length: isize, encoding: u32) -> isize;
        fn CFStringGetCString(theString: *mut c_void, buffer: *mut u8, bufferSize: isize, encoding: u32) -> bool;
        fn CFStringCreateWithCString(alloc: *mut c_void, c_str: *const i8, encoding: u32) -> *mut c_void;
    }

    pub fn init_timeout() {
        unsafe {
            let sys = AXUIElementCreateSystemWide();
            if !sys.is_null() {
                AXUIElementSetMessagingTimeout(sys, 0.05);
                CFRelease(sys);
            }
        }
    }

    fn cf_str(s: &str) -> *mut c_void {
        let Ok(c) = CString::new(s) else { return std::ptr::null_mut() };
        unsafe { CFStringCreateWithCString(std::ptr::null_mut(), c.as_ptr(), K_CF_STRING_ENCODING_UTF8) }
    }

    fn cf_to_string(cf: *mut c_void) -> Option<String> {
        if cf.is_null() { return None; }
        unsafe {
            let len = CFStringGetLength(cf);
            let max = CFStringGetMaximumSizeForEncoding(len, K_CF_STRING_ENCODING_UTF8) + 1;
            let mut buf = vec![0u8; max as usize];
            if CFStringGetCString(cf, buf.as_mut_ptr(), max, K_CF_STRING_ENCODING_UTF8) {
                CStr::from_ptr(buf.as_ptr() as *const i8).to_str().ok().map(|s| s.to_string())
            } else {
                None
            }
        }
    }

    unsafe fn copy_attr(element: AXUIElementRef, attr: &str) -> Option<*mut c_void> {
        let key = cf_str(attr);
        if key.is_null() { return None; }
        let mut val: *mut c_void = std::ptr::null_mut();
        let err = AXUIElementCopyAttributeValue(element, key, &mut val);
        CFRelease(key);
        if err == K_AX_ERROR_SUCCESS && !val.is_null() { Some(val) } else { None }
    }

    pub fn get_selected_text() -> Option<String> {
        if !unsafe { AXIsProcessTrustedWithOptions(std::ptr::null_mut()) } {
            return None;
        }
        unsafe {
            let sys = AXUIElementCreateSystemWide();
            if sys.is_null() { return None; }
            let focused = copy_attr(sys, "AXFocusedUIElement");
            CFRelease(sys);
            let focused = focused?;
            let selected = copy_attr(focused, "AXSelectedText");
            CFRelease(focused);
            let selected = selected?;
            if CFGetTypeID(selected) != CFStringGetTypeID() {
                CFRelease(selected);
                return None;
            }
            let text = cf_to_string(selected);
            CFRelease(selected);
            text.filter(|s| !s.trim().is_empty())
        }
    }
}

// ============================================================================
// Layer 2: CGEventPostToPid + NSPasteboard 轮询
// ============================================================================

#[cfg(target_os = "macos")]
fn read_clipboard_ns() -> Option<String> {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
    unsafe { NSPasteboard::generalPasteboard().stringForType(NSPasteboardTypeString).map(|s| s.to_string()) }
}

#[cfg(target_os = "macos")]
fn clipboard_change_count() -> isize {
    use objc2_app_kit::NSPasteboard;
    NSPasteboard::generalPasteboard().changeCount()
}

#[cfg(target_os = "macos")]
pub struct PasteboardSnapshot {
    pub change_count: isize,
    items: Vec<Vec<(String, Vec<u8>)>>,
}

#[cfg(target_os = "macos")]
pub fn snapshot_clipboard() -> PasteboardSnapshot {
    use objc2_app_kit::NSPasteboard;
    let pb = NSPasteboard::generalPasteboard();
    let change_count = pb.changeCount();
    let mut items = Vec::new();
    if let Some(ns_items) = pb.pasteboardItems() {
        for item in ns_items.iter() {
            let mut entries = Vec::new();
            for t in item.types().iter() {
                if let Some(data) = item.dataForType(&t) {
                    entries.push((t.to_string(), data.to_vec()));
                }
            }
            items.push(entries);
        }
    }
    PasteboardSnapshot { change_count, items }
}

#[cfg(target_os = "macos")]
fn restore_clipboard(snap: &PasteboardSnapshot) {
    use objc2::runtime::ProtocolObject;
    use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardWriting};
    use objc2_foundation::{NSArray, NSData, NSString};
    let pb = NSPasteboard::generalPasteboard();
    pb.clearContents();
    if snap.items.is_empty() { return; }
    let mut protos: Vec<objc2::rc::Retained<ProtocolObject<dyn NSPasteboardWriting>>> = Vec::new();
    for entries in &snap.items {
        let item = NSPasteboardItem::new();
        for (t, bytes) in entries {
            let ns_type = NSString::from_str(t);
            let ns_data = NSData::with_bytes(bytes);
            item.setData_forType(&ns_data, &ns_type);
        }
        protos.push(ProtocolObject::from_retained(item));
    }
    let array = NSArray::from_retained_slice(&protos);
    pb.writeObjects(&array);
}

/// 在主线程调用：向目标 PID 注入 Cmd+C。
#[cfg(target_os = "macos")]
pub fn inject_copy(pid: i32) {
    use core_graphics::event::{CGEvent, CGEventFlags, CGKeyCode};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    const KEY_C: CGKeyCode = 0x08;
    const CMD: u64 = 1 << 20;
    let Ok(src) = CGEventSource::new(CGEventSourceStateID::Private) else { return };
    if let Ok(dn) = CGEvent::new_keyboard_event(src.clone(), KEY_C, true) {
        dn.set_flags(CGEventFlags::from_bits_retain(CMD));
        dn.post_to_pid(pid as libc::pid_t);
        std::thread::sleep(Duration::from_millis(10));
        if let Ok(up) = CGEvent::new_keyboard_event(src, KEY_C, false) {
            up.set_flags(CGEventFlags::from_bits_retain(CMD));
            up.post_to_pid(pid as libc::pid_t);
        }
    }
}

/// 在后台线程调用：轮询剪贴板变化，读取文本后恢复原内容。
#[cfg(target_os = "macos")]
pub fn poll_clipboard(snap: PasteboardSnapshot) -> String {
    log(&format!("[poll_clipboard] start, change_count={}", snap.change_count));
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(500) {
        std::thread::sleep(Duration::from_millis(10));
        if clipboard_change_count() != snap.change_count {
            // 等稳定
            let mut last = clipboard_change_count();
            let mut stable = Instant::now();
            while stable.elapsed() < Duration::from_millis(30) {
                std::thread::sleep(Duration::from_millis(5));
                let cur = clipboard_change_count();
                if cur != last { last = cur; stable = Instant::now(); }
            }
            let text = read_clipboard_ns().unwrap_or_default();
            let text = text.trim().to_string();
            restore_clipboard(&snap);
            log(&format!("[poll_clipboard] got text in {:?}: {:?}", start.elapsed(), text));
            return text;
        }
    }
    restore_clipboard(&snap);
    log("[poll_clipboard] timeout, no clipboard change");
    String::new()
}

// ============================================================================
// 公开 API
// ============================================================================

#[cfg(target_os = "macos")]
pub fn init_ax_timeout() {
    ax::init_timeout();
}

pub fn log(msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/voidnix-ts.log") {
        let _ = writeln!(f, "{}", msg);
    }
}

/// Layer 1 only（在后台线程调用）。
#[cfg(target_os = "macos")]
pub fn try_ax() -> Option<String> {
    log("[try_ax] called");
    let result = ax::get_selected_text();
    log(&format!("[try_ax] result: {:?}", result));
    result
}

/// post_key_to_pid 供其他模块使用。
#[cfg(target_os = "macos")]
pub fn post_key_to_pid(pid: i32, key_code: u16, flags: u64) {
    use core_graphics::event::{CGEvent, CGEventFlags};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    let Ok(src) = CGEventSource::new(CGEventSourceStateID::Private) else { return };
    if let Ok(dn) = CGEvent::new_keyboard_event(src.clone(), key_code, true) {
        dn.set_flags(CGEventFlags::from_bits_retain(flags));
        dn.post_to_pid(pid as libc::pid_t);
        std::thread::sleep(Duration::from_millis(10));
        if let Ok(up) = CGEvent::new_keyboard_event(src, key_code, false) {
            up.set_flags(CGEventFlags::from_bits_retain(flags));
            up.post_to_pid(pid as libc::pid_t);
        }
    }
}
