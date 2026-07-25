/// 屏幕录制权限检查：CGPreflightScreenCaptureAccess 纳秒级 TCC 查询，不触发截屏。
#[tauri::command]
pub fn check_screen_recording_permission() -> bool {
    crate::platform::permission::check_screen_recording()
}

#[tauri::command]
pub fn check_accessibility_permission() -> bool {
    crate::platform::permission::check_accessibility()
}

#[tauri::command]
pub fn request_accessibility_permission() -> bool {
    crate::platform::permission::request_accessibility()
}

/// 全磁盘访问权限检查：读 ~/Desktop（受 TCC 保护），微秒级 syscall。
#[tauri::command]
pub fn check_full_disk_access_permission() -> bool {
    crate::platform::permission::check_full_disk_access()
}

#[tauri::command]
pub fn open_privacy_settings(kind: String) {
    crate::platform::permission::open_privacy_settings(&kind);
}
