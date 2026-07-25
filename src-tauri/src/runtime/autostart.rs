/// 是否已开启开机自启（SMAppService status == Enabled）。
#[tauri::command]
pub fn is_autostart_enabled() -> bool {
    #[cfg(target_os = "macos")]
    return crate::platform::autostart::is_enabled();
    #[cfg(not(target_os = "macos"))]
    return false;
}

/// 开启开机自启（注册为系统 Login Item，出现在系统设置登录项列表）。
/// 失败时返回 macOS NSError 描述，前端据此显示真实原因。
#[tauri::command]
pub fn enable_autostart() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return crate::platform::autostart::enable();
    #[cfg(not(target_os = "macos"))]
    return Err("当前平台不支持开机自启".to_string());
}

/// 关闭开机自启。
#[tauri::command]
pub fn disable_autostart() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return crate::platform::autostart::disable();
    #[cfg(not(target_os = "macos"))]
    return Err("当前平台不支持开机自启".to_string());
}
