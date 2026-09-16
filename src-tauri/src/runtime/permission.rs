/// 屏幕录制权限检查：CGPreflightScreenCaptureAccess 纳秒级 TCC 查询，不触发截屏。
#[tauri::command]
pub fn check_screen_recording_permission() -> bool {
    crate::platform::permission::check_screen_recording()
}

/// 应用公证状态（本地检测 + 缓存）：未公证时辅助功能/屏幕录制的 API 请求路径
/// 写入的 TCC 条目无效，授权入口据此分流（走系统设置手动添加）。首次为 xcrun
/// 子进程检测（数百 ms），spawn_blocking 防主线程停顿（同步命令在主线程执行）。
#[tauri::command]
pub async fn check_app_notarized() -> bool {
    tauri::async_runtime::spawn_blocking(crate::platform::permission::check_app_notarized)
        .await
        .unwrap_or(false)
}

#[tauri::command]
pub fn check_accessibility_permission() -> bool {
    crate::platform::permission::check_accessibility()
}

#[tauri::command]
pub fn request_accessibility_permission() -> bool {
    crate::platform::permission::request_accessibility()
}

/// 屏幕录制权限请求：CGRequestScreenCaptureAccess 弹系统对话框并注册进系统设置
/// 列表，同步阻塞至用户响应（可达数分钟）——async + spawn_blocking 在阻塞线程池
/// 执行（同步命令在主线程执行，会冻结 NSApplication runloop，勿改回）。
#[tauri::command]
pub async fn request_screen_recording_permission() -> bool {
    tauri::async_runtime::spawn_blocking(crate::platform::permission::request_screen_recording)
        .await
        .unwrap_or(false)
}

/// 打开系统设置对应隐私面板并启动授权会话：设置激活置顶（默认聚焦）、主窗避让到
/// 设置窗口旁并钉住至授权完成（1s 轮询，完成/10min 超时 emit `perm-flow`）。
#[tauri::command]
pub fn open_privacy_settings(app: tauri::AppHandle, kind: String) {
    crate::platform::permission::open_privacy_settings(&app, &kind);
}

/// 录屏手动添加拖拽指引浮窗（独立原生小窗，悬浮于设置窗口底部中心内侧）：单行
/// 说明 + 应用图标原生拖拽源，拖入系统设置列表等价点 + 添加（HTML5 拖拽无法跨
/// 应用携带 file URL）。文案由前端传参承载 i18n，显隐随授权会话驱动。
#[tauri::command]
pub fn show_perm_drag_hint(app: tauri::AppHandle, text: String) {
    crate::platform::perm_drag::show(&app, &text);
}

#[tauri::command]
pub fn hide_perm_drag_hint(app: tauri::AppHandle) {
    crate::platform::perm_drag::hide(&app);
}

/// 全磁盘访问权限检查：open 系统级 TCC 数据库（仅 FDA 可读），
/// 不触碰桌面/下载等「文件与文件夹」管辖目录（首次触碰会触发系统询问弹窗）。
#[tauri::command]
pub fn check_full_disk_access_permission() -> bool {
    crate::platform::permission::check_full_disk_access()
}
