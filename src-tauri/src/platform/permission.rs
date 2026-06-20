/// 屏幕录制权限：尝试调用 CGDisplayCreateImage，成功则返回 true。
#[cfg(target_os = "macos")]
pub fn check_screen_recording() -> bool {
    use core_graphics::display::CGDisplay;
    CGDisplay::main().image().is_some()
}

#[cfg(not(target_os = "macos"))]
pub fn check_screen_recording() -> bool {
    false
}

/// 辅助功能权限：调用 AXIsProcessTrusted 检查。
#[cfg(target_os = "macos")]
pub fn check_accessibility() -> bool {
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }
    unsafe { AXIsProcessTrusted() }
}

#[cfg(not(target_os = "macos"))]
pub fn check_accessibility() -> bool {
    false
}

/// 请求辅助功能权限（弹出系统授权对话框）。
/// 返回值表示授权是否已成功。
#[cfg(target_os = "macos")]
pub fn request_accessibility() -> bool {
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::string::CFString;
    use std::ffi::c_void;
    extern "C" {
        fn AXIsProcessTrustedWithOptions(options: *mut c_void) -> bool;
    }
    let key = CFString::new("AXTrustedCheckOptionPrompt");
    let val = CFBoolean::true_value();
    let dict: CFDictionary<CFString, CFBoolean> = CFDictionary::from_CFType_pairs(&[(key, val)]);
    unsafe { AXIsProcessTrustedWithOptions(dict.as_concrete_TypeRef() as *mut c_void) }
}

#[cfg(not(target_os = "macos"))]
pub fn request_accessibility() -> bool {
    false
}

/// 全磁盘访问权限：尝试列出用户桌面目录，成功说明有文件访问权限。
/// macOS 的 FDA 没有直接检查 API，通过试读受保护目录来判断。
#[cfg(target_os = "macos")]
pub fn check_full_disk_access() -> bool {
    // 尝试读取 ~/Desktop 目录（受 TCC 保护）
    if let Some(home) = dirs::home_dir() {
        let desktop = home.join("Desktop");
        std::fs::read_dir(&desktop).is_ok()
    } else {
        false
    }
}

#[cfg(not(target_os = "macos"))]
pub fn check_full_disk_access() -> bool {
    false
}

/// 打开系统设置中对应隐私面板。
/// kind: "accessibility" | "screen_recording" | "full_disk_access"
#[cfg(target_os = "macos")]
pub fn open_privacy_settings(kind: &str) {
    use std::process::Command;
    let url = match kind {
        "accessibility" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
        }
        "screen_recording" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
        }
        "full_disk_access" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles"
        }
        _ => {
            let _ = Command::new("open")
                .args(["-b", "com.apple.systempreferences"])
                .spawn();
            return;
        }
    };
    let _ = Command::new("open").arg(url).spawn();
}

#[cfg(not(target_os = "macos"))]
pub fn open_privacy_settings(_kind: &str) {}
