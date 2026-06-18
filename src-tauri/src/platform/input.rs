//! CGEvent 键盘注入统一接口。
//!
//! 替代原 text_selection::post_key_to_pid / inject_copy / clipboard::simulate_cmd_v 三套实现。
//! Phase 2 将让 clipboard 扩展的 simulate_cmd_v 也委托至此。

#![allow(dead_code)]

use std::time::Duration;

/// CGEvent 键盘注入：向目标 PID 发送按键事件。
///
/// 统一替代原 text_selection::post_key_to_pid / inject_copy / clipboard::simulate_cmd_v。
pub fn post_key(key_code: u16, flags: u64, pid: i32) {
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

/// 常用 CGEvent flag 常量（与 macOS CGEventFlags 对齐）。
pub const FLAG_CMD: u64 = 1 << 20;
pub const FLAG_SHIFT: u64 = 1 << 17;
pub const FLAG_OPT: u64 = 1 << 19;
pub const FLAG_CTRL: u64 = 1 << 18;

/// macOS key codes。
pub const KEY_C: u16 = 0x08;
pub const KEY_V: u16 = 0x09;
pub const KEY_PERIOD: u16 = 0x2f;
pub const KEY_RETURN: u16 = 0x24;

/// 向目标 PID 注入 Cmd+C（复制）。
pub fn inject_copy(pid: i32) {
    post_key(KEY_C, FLAG_CMD, pid);
}

/// 向目标 PID 注入 Cmd+V（粘贴）。
pub fn inject_paste(pid: i32) {
    post_key(KEY_V, FLAG_CMD, pid);
}

/// 全局注入按键（发送到系统事件 tap，影响当前焦点应用）。
pub fn post_key_global(key_code: u16, flags: u64) {
    use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    let Ok(src) = CGEventSource::new(CGEventSourceStateID::CombinedSessionState) else {
        return;
    };
    if let Ok(dn) = CGEvent::new_keyboard_event(src.clone(), key_code, true) {
        dn.set_flags(CGEventFlags::from_bits_retain(flags));
        dn.post(CGEventTapLocation::HID);
        std::thread::sleep(Duration::from_millis(20));
        if let Ok(up) = CGEvent::new_keyboard_event(src, key_code, false) {
            up.set_flags(CGEventFlags::from_bits_retain(flags));
            up.post(CGEventTapLocation::HID);
        }
    }
}

/// 全局注入 Cmd+V（粘贴到当前焦点应用）。
pub fn paste_global() {
    post_key_global(KEY_V, FLAG_CMD);
}
