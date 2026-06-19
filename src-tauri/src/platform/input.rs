//! CGEvent 键盘注入统一接口（RV §2.6）。
//!
//! 统一替代原 post_key_to_pid / inject_copy / simulate_cmd_v 三套实现：
//! - `post_key` 是原语（虚拟键码 + 修饰键 + 可选目标 pid）；
//! - `post_combo` 是字符串糖（`"cmd+c"`、`"cmd+shift+."`），内部委托 post_key。

use std::time::Duration;

/// 修饰键（不含 Fn：macOS 上 Fn 是硬件键非修饰键，RV §2.6）。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Modifier {
    Cmd,
    Shift,
    Opt,
    Ctrl,
}

impl Modifier {
    /// 对应 macOS CGEventFlags 位。
    const fn flag(self) -> u64 {
        match self {
            Modifier::Cmd => 1 << 20,
            Modifier::Shift => 1 << 17,
            Modifier::Opt => 1 << 19,
            Modifier::Ctrl => 1 << 18,
        }
    }
}

/// CGEvent 键盘注入原语：发送单键 + 修饰键。
///
/// `pid = Some(p)` 注入到目标进程（Private 事件源 + `post_to_pid`）；
/// `pid = None` 全局注入（CombinedSessionState 事件源 + 事件 tap，影响当前焦点应用）。
pub fn post_key(key_code: u16, modifiers: &[Modifier], pid: Option<i32>) {
    use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    let flags = modifiers.iter().copied().fold(0u64, |acc, m| acc | m.flag());
    let (state, to_pid) = match pid {
        Some(p) => (CGEventSourceStateID::Private, Some(p)),
        None => (CGEventSourceStateID::CombinedSessionState, None),
    };
    let Ok(src) = CGEventSource::new(state) else { return };
    // 全局路径（事件 tap 调度）需更长间隔；目标 pid 路径较短。
    let interval = if to_pid.is_some() { 10 } else { 20 };
    let post = |ev: &CGEvent, pid: Option<i32>| match pid {
        Some(p) => ev.post_to_pid(p as libc::pid_t),
        None => ev.post(CGEventTapLocation::HID),
    };

    if let Ok(dn) = CGEvent::new_keyboard_event(src.clone(), key_code, true) {
        dn.set_flags(CGEventFlags::from_bits_retain(flags));
        post(&dn, to_pid);
        std::thread::sleep(Duration::from_millis(interval));
        if let Ok(up) = CGEvent::new_keyboard_event(src, key_code, false) {
            up.set_flags(CGEventFlags::from_bits_retain(flags));
            post(&up, to_pid);
        }
    }
}

/// 组合键字符串注入：解析 `"cmd+c"`、`"cmd+shift+."`、`"return"` 等并调 [`post_key`]。
///
/// `pid` 语义同 [`post_key`]。修饰键名大小写不敏感（cmd/shift/opt/ctrl，支持别名 command/alt/option/control）；
/// 键名为单字符（a-z / 0-9 / 常见标点）或命名键（return/enter/tab/space/esc/delete/up/down/left/right）。
/// 无法解析的 token 静默放弃（CGEvent 注入本就 best-effort）。
pub fn post_combo(combo: &str, pid: Option<i32>) {
    let tokens: Vec<&str> = combo.split('+').map(str::trim).collect();
    let Some((key_token, mod_tokens)) = tokens.split_last() else { return };
    let mut modifiers = Vec::with_capacity(mod_tokens.len());
    for t in mod_tokens {
        let m = match t.to_ascii_lowercase().as_str() {
            "cmd" | "command" => Modifier::Cmd,
            "shift" => Modifier::Shift,
            "opt" | "option" | "alt" => Modifier::Opt,
            "ctrl" | "control" => Modifier::Ctrl,
            _ => return,
        };
        modifiers.push(m);
    }
    let Some(key_code) = parse_key_code(key_token) else { return };
    post_key(key_code, &modifiers, pid);
}

/// 键名 → macOS 虚拟键码（QWERTY）。单字符与命名键的单一映射源。
fn parse_key_code(name: &str) -> Option<u16> {
    Some(match name.to_ascii_lowercase().as_str() {
        // 命名键
        "return" | "enter" => 0x24,
        "tab" => 0x30,
        "space" => 0x31,
        "esc" | "escape" => 0x35,
        "delete" | "backspace" => 0x33,
        "up" => 0x7E,
        "down" => 0x7D,
        "left" => 0x7B,
        "right" => 0x7C,
        // 标点
        "." => 0x2F,
        "," => 0x2B,
        "/" => 0x2C,
        ";" => 0x29,
        "'" => 0x27,
        "[" => 0x21,
        "]" => 0x1E,
        "\\" => 0x2A,
        "-" => 0x1B,
        "=" => 0x18,
        "`" => 0x32,
        // 字母（QWERTY 布局）
        "a" => 0x00, "s" => 0x01, "d" => 0x02, "f" => 0x03, "h" => 0x04, "g" => 0x05,
        "z" => 0x06, "x" => 0x07, "c" => 0x08, "v" => 0x09, "b" => 0x0B, "q" => 0x0C,
        "w" => 0x0D, "e" => 0x0E, "r" => 0x0F, "y" => 0x10, "t" => 0x11, "u" => 0x20,
        "i" => 0x22, "o" => 0x1F, "p" => 0x23, "l" => 0x25, "j" => 0x26, "k" => 0x28,
        "n" => 0x2D, "m" => 0x2E,
        // 数字（顶排）
        "1" => 0x12, "2" => 0x13, "3" => 0x14, "4" => 0x15, "5" => 0x17, "6" => 0x16,
        "7" => 0x1A, "8" => 0x1C, "9" => 0x19, "0" => 0x1D,
        _ => return None,
    })
}
