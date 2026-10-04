// 原生 toast 编排：前端 showToast 薄壳经 show_toast 命令入队，platform::toast
// 渲染（独立 NSPanel，主窗隐藏不影响展示）。锚定所在屏顶部中心（HUD 位，与
// 主窗显隐/坐标无关）。到期调度在 platform 侧用 NSTimer 主线程原生定时（同
// command 上下文创建，渲染管线正常提交）。

/// 默认展示时长（ms），与原 web 版一致。
const DEFAULT_DURATION_MS: u64 = 2000;

/// 显示一条原生 toast。sync command 在主线程执行（同 get_main_frame），
/// NSWindow 读取与 panel 操作安全；kind 仅区分 error（语义红图标）。
#[tauri::command]
pub fn show_toast(
    app: tauri::AppHandle,
    message: String,
    kind: Option<String>,
    duration_ms: Option<u64>,
) {
    #[cfg(target_os = "macos")]
    {
        let kind_error = kind.as_deref() == Some("error");
        let duration = duration_ms.unwrap_or(DEFAULT_DURATION_MS);
        if objc2_foundation::MainThreadMarker::new().is_none() {
            // 防御兜底：sync command 理论恒主线程，此处仅防未来变化丢反馈
            let a = app.clone();
            let msg = message.clone();
            let _ =
                app.run_on_main_thread(move || show_toast_on_main(&a, &msg, kind_error, duration));
            return;
        }
        show_toast_on_main(&app, &message, kind_error, duration);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, message, kind, duration_ms);
    }
}

#[cfg(target_os = "macos")]
fn show_toast_on_main(app: &tauri::AppHandle, message: &str, kind_error: bool, duration: u64) {
    use tauri::Manager;

    let Some(mtm) = objc2_foundation::MainThreadMarker::new() else {
        return;
    };

    // 锚定屏：主窗 placement 屏优先、否则光标屏（与主窗呈现在同一块屏，反馈贴
    // 动作发生屏）。顶部中心锚定与主窗显隐/坐标无关——无竞态面。到期调度
    //（NSTimer）在 platform 侧随 show_row 一并创建。
    let Some(vis) = app
        .get_webview_window("main")
        .and_then(|win| crate::platform::window::main_frame_and_vis(&win).map(|(_, v)| v))
    else {
        // 无主窗且光标屏不可得（无头环境），放弃反馈
        return;
    };
    crate::platform::toast::show_row(mtm, message, kind_error, duration, vis);
}
