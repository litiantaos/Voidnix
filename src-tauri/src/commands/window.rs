// 主窗口尺寸调整命令。转发到 webkit_tuning::resize_main。
// T11 实装：接入 webkit_tuning 顶层入口。

#[tauri::command]
pub fn set_main_window_size(
    app: tauri::AppHandle,
    width: f64,
    height: f64,
) -> Result<(), String> {
    crate::webkit_tuning::resize_main(&app, width, height)
}
