//! macOS 选中文本提取（AX 系统级 API）。
//!
//! 与 platform::input（键盘注入）+ platform::pasteboard（剪贴板操作）配合使用。
//! 划词取词流程：try_ax → 失败则 input::post_combo("cmd+c") → poll_clipboard。

use std::ffi::{c_void, CStr, CString};
use std::time::{Duration, Instant};

use crate::platform::pasteboard;

// ── AX system-wide 选中文本提取 ─────────────────────────

#[cfg(target_os = "macos")]
mod ax {
    use super::*;

    type AXUIElementRef = *mut c_void;
    type AXError = i32;
    const K_AX_ERROR_SUCCESS: AXError = 0;
    const K_CF_STRING_ENCODING_UTF8: u32 = 0x08000100;

    /// M-rs3：缓存 system-wide AXUIElement。
    /// AXUIElementSetMessagingTimeout 是 per-element 设置（非进程级全局），
    /// 故 init_timeout 创建-设-释放后 timeout 即失效。改为进程生命期缓存 element，
    /// 让所有 get_selected_text 复用同一已设 timeout 的句柄。
    struct SystemWideAx(AXUIElementRef);
    // SAFETY: AXUIElementRef 是 Apple AX API 的不可变句柄，跨线程读取安全
    unsafe impl Send for SystemWideAx {}
    unsafe impl Sync for SystemWideAx {}

    static SYSTEM_WIDE: std::sync::OnceLock<SystemWideAx> = std::sync::OnceLock::new();

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXUIElementCreateSystemWide() -> AXUIElementRef;
        fn AXUIElementSetMessagingTimeout(
            element: AXUIElementRef,
            timeout_in_seconds: f32,
        ) -> AXError;
        fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: *mut c_void,
            value: *mut *mut c_void,
        ) -> AXError;
        fn AXIsProcessTrustedWithOptions(options: *mut c_void) -> bool;
        fn CFGetTypeID(cf: *mut c_void) -> usize;
        fn CFStringGetTypeID() -> usize;
        fn CFRelease(cf: *mut c_void);
        fn CFStringGetLength(theString: *mut c_void) -> isize;
        fn CFStringGetMaximumSizeForEncoding(length: isize, encoding: u32) -> isize;
        fn CFStringGetCString(
            theString: *mut c_void,
            buffer: *mut u8,
            bufferSize: isize,
            encoding: u32,
        ) -> bool;
        fn CFStringCreateWithCString(
            alloc: *mut c_void,
            c_str: *const i8,
            encoding: u32,
        ) -> *mut c_void;
    }

    /// 取（首次创建并设 timeout）缓存的 system-wide element。
    /// 返回的 AXUIElementRef 进程生命期常驻，无需调用方释放。
    fn system_wide() -> AXUIElementRef {
        SYSTEM_WIDE
            .get_or_init(|| {
                // SAFETY: AXUIElementCreateSystemWide 无副作用依赖，可跨线程调用
                let sys = unsafe { AXUIElementCreateSystemWide() };
                if !sys.is_null() {
                    unsafe { AXUIElementSetMessagingTimeout(sys, 0.05) };
                }
                SystemWideAx(sys)
            })
            .0
    }

    pub fn init_timeout() {
        // M-rs3：触发 system_wide 初始化（首次调用设 timeout 并缓存 element）
        system_wide();
    }

    fn cf_str(s: &str) -> *mut c_void {
        let Ok(c) = CString::new(s) else {
            return std::ptr::null_mut();
        };
        unsafe {
            CFStringCreateWithCString(std::ptr::null_mut(), c.as_ptr(), K_CF_STRING_ENCODING_UTF8)
        }
    }

    fn cf_to_string(cf: *mut c_void) -> Option<String> {
        if cf.is_null() {
            return None;
        }
        unsafe {
            let len = CFStringGetLength(cf);
            let max = CFStringGetMaximumSizeForEncoding(len, K_CF_STRING_ENCODING_UTF8) + 1;
            let mut buf = vec![0u8; max as usize];
            if CFStringGetCString(cf, buf.as_mut_ptr(), max, K_CF_STRING_ENCODING_UTF8) {
                CStr::from_ptr(buf.as_ptr() as *const i8)
                    .to_str()
                    .ok()
                    .map(|s| s.to_string())
            } else {
                None
            }
        }
    }

    unsafe fn copy_attr(element: AXUIElementRef, attr: &str) -> Option<*mut c_void> {
        let key = cf_str(attr);
        if key.is_null() {
            return None;
        }
        let mut val: *mut c_void = std::ptr::null_mut();
        let err = AXUIElementCopyAttributeValue(element, key, &mut val);
        CFRelease(key);
        if err == K_AX_ERROR_SUCCESS && !val.is_null() {
            Some(val)
        } else {
            None
        }
    }

    pub fn get_selected_text() -> Option<String> {
        if !unsafe { AXIsProcessTrustedWithOptions(std::ptr::null_mut()) } {
            return None;
        }
        let sys = system_wide();
        if sys.is_null() {
            return None;
        }
        unsafe {
            // M-rs3：复用缓存 system-wide element（已设 timeout），不再每次创建+释放
            let focused = copy_attr(sys, "AXFocusedUIElement")?;
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

// ── 公开 API ──────────────────────────────────────────

/// 初始化 AX 超时设置（setup 阶段调用一次）。
#[cfg(target_os = "macos")]
pub fn init_ax_timeout() {
    ax::init_timeout();
}

/// Layer 1：通过 AX 提取系统级选中文本（后台线程调用）。
#[cfg(target_os = "macos")]
pub fn try_ax() -> Option<String> {
    ax::get_selected_text()
}

/// Layer 2：在后台线程轮询剪贴板变化（配合 input::post_combo("cmd+c") 使用）。
///
/// 流程：post_combo("cmd+c") 注入复制后调用此函数 → 等待 changeCount 变化 → 读取文本 → 恢复原剪贴板。
#[cfg(target_os = "macos")]
pub fn poll_clipboard(snap: pasteboard::PasteboardSnapshot) -> String {
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(500) {
        std::thread::sleep(Duration::from_millis(10));
        if pasteboard::change_count() != snap.change_count {
            let mut last = pasteboard::change_count();
            let mut stable = Instant::now();
            while stable.elapsed() < Duration::from_millis(30) {
                std::thread::sleep(Duration::from_millis(5));
                let cur = pasteboard::change_count();
                if cur != last {
                    last = cur;
                    stable = Instant::now();
                }
            }
            let text = pasteboard::read_text().unwrap_or_default();
            let text = text.trim().to_string();
            pasteboard::restore(&snap);
            return text;
        }
    }
    pasteboard::restore(&snap);
    String::new()
}
