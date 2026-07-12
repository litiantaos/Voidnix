use crate::runtime::registry::Extension;
use serde::{Deserialize, Serialize};

mod window_snap;

/// 自定义尺寸 floor/cap（权威源，TS config.ts BOUNDS 须手动同步，
/// check:wm-bounds CI 强制约束）。
const WIDTH_BOUNDS: (f64, f64) = (200.0, 4096.0);
const HEIGHT_BOUNDS: (f64, f64) = (200.0, 4096.0);

/// Window manager 扩展。
pub struct WindowManagerExtension;

/// 命令注册（局部 invoke_handler）。
pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new("window-manager").build()
}

#[async_trait::async_trait]
impl Extension for WindowManagerExtension {
    fn id(&self) -> &'static str {
        "window-manager"
    }

    async fn setup(&self, app: &tauri::AppHandle) -> tauri::Result<()> {
        configure_snap_panel(app);
        Ok(())
    }
}

/// snap-panel 窗口配置：Mica 材质底 + NonactivatingPanel + 跨 Space + 原生阴影。
/// 窗口尺寸精简为面板大小（show_panel 定位），CSS box-shadow 会被窗口裁剪故用原生阴影。
#[cfg(target_os = "macos")]
fn configure_snap_panel(app: &tauri::AppHandle) {
    use objc2_app_kit::{NSWindow as SnapNSWindow, NSWindowCollectionBehavior};
    use tauri::Manager;
    let Some(window) = app.get_webview_window("snap-panel") else {
        return;
    };
    let Ok(raw) = window.ns_window() else { return };
    let raw = raw.cast::<SnapNSWindow>();
    let Some(ns_window) = (unsafe { raw.as_ref() }) else {
        return;
    };
    // Mica 材质底（contentView 圆角 10 + NSVisualEffectView + 子视图非透明 + 窗口本体透明）
    crate::platform::window::apply_mica_material(ns_window, 10.0);
    // SAFETY: raw 来自 window.ns_window()（上方 Ok 分支保证句柄有效）；ns_window 经
    // as_ref Some 分支二次非空校验。所有 msg_send 均为 NSWindow 标准选择子
    // （setLevel:/setCollectionBehavior/setAlphaValue/orderFrontRegardless/setHasShadow:
    // /setAcceptsMouseMovedEvents:），参数类型匹配。convert_to_panel 内部自管 null 检查。
    unsafe {
        crate::platform::panel::convert_to_panel(raw.cast());
        ns_window.setLevel(objc2_app_kit::NSStatusWindowLevel + 1);
        let behavior = NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Transient
            | NSWindowCollectionBehavior::CanJoinAllSpaces;
        ns_window.setCollectionBehavior(behavior);
        ns_window.setIgnoresMouseEvents(true);
        let _: () = objc2::msg_send![ns_window, setAcceptsMouseMovedEvents: true];
        ns_window.setAlphaValue(0.0);
        ns_window.orderFrontRegardless();
        ns_window.setHasShadow(true);
    }
}

#[cfg(not(target_os = "macos"))]
fn configure_snap_panel(_app: &tauri::AppHandle) {}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ScreenInfo {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub is_main: bool,
}

mod platform;

#[tauri::command]
pub fn get_screen_info() -> Vec<ScreenInfo> {
    platform::do_get_screens()
}

#[tauri::command]
pub async fn set_frontmost_window_layout(
    app: tauri::AppHandle,
    layout: String,
    custom_width: Option<f64>,
    custom_height: Option<f64>,
    prev_pid: Option<i32>,
) -> Result<(), String> {
    platform::do_set_layout(
        &app,
        &layout,
        custom_width.unwrap_or(1200.0),
        custom_height.unwrap_or(800.0),
        prev_pid,
    )
}

#[tauri::command]
pub fn check_window_manager_accessibility() -> bool {
    platform::do_check_accessibility()
}

#[tauri::command]
pub async fn set_window_manager_enabled(
    app: tauri::AppHandle,
    enabled: bool,
) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let app_clone = app.clone();
    app.run_on_main_thread(move || {
        platform::do_set_window_manager_enabled(&app_clone, enabled);
        let _ = tx.send(());
    })
    .map_err(|e| e.to_string())?;
    rx.recv_timeout(std::time::Duration::from_secs(10))
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn set_snap_size(width: f64, height: f64) {
    platform::do_set_snap_size(width, height);
}

#[tauri::command]
pub async fn show_snap_panel(app: tauri::AppHandle) -> Result<(), String> {
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    use tauri::Manager;
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let app_clone = app.clone();
    app.run_on_main_thread(move || {
        if let Some(w) = app_clone.get_webview_window("snap-panel") {
            let mut target_frame: Option<NSRect> = None;
            let (pw, ph) = window_snap::panel_dimensions();
            // SAFETY: ns_window 经 Ok 分支有效；ns 经 as_ref Some 非空校验。
            // setFrame_display/setAlphaValue/makeKeyAndOrderFront 均为 NSWindow 标准方法。
            // 窗口目标位置已由 show_panel（drag monitor）设为面板尺寸。
            unsafe {
                if let Ok(raw) = w.ns_window() {
                    if let Some(ns) = raw.cast::<objc2_app_kit::NSWindow>().as_ref() {
                        // 固定 target 尺寸（不读 ns.frame() 的 size，避免前次动画未完成时漂移）；
                        // 中心点取当前 frame（动画为中心对齐缩放，中心点稳定）
                        let cur = ns.frame();
                        let cx = cur.origin.x + cur.size.width / 2.0;
                        let cy = cur.origin.y + cur.size.height / 2.0;
                        // 起点：固定 80%，中心对齐
                        let sw = pw * 0.8;
                        let sh = ph * 0.8;
                        ns.setFrame_display(
                            NSRect::new(
                                NSPoint::new(cx - sw / 2.0, cy - sh / 2.0),
                                NSSize::new(sw, sh),
                            ),
                            false,
                        );
                        ns.setAlphaValue(0.0);
                        // panel 已是 NonactivatingPanel：makeKey 只让面板接收事件，
                        // 不会把 Voidnix 拉成前台 app，前台应用焦点保持不变。
                        let _: () = objc2::msg_send![
                            ns,
                            makeKeyAndOrderFront: std::ptr::null::<objc2::runtime::AnyObject>()
                        ];
                        // target：固定尺寸，中心对齐
                        target_frame = Some(NSRect::new(
                            NSPoint::new(cx - pw / 2.0, cy - ph / 2.0),
                            NSSize::new(pw, ph),
                        ));
                    }
                }
            }
            // alpha + frame 同步动画（缩小→目标 + alpha 0→1，单 group）
            if let Some(tf) = target_frame {
                crate::platform::window::animate_panel(
                    &w,
                    1.0,
                    tf.origin.x,
                    tf.origin.y,
                    tf.size.width,
                    tf.size.height,
                    0.25,
                );
            }
        }
        let _ = tx.send(());
    })
    .map_err(|e| e.to_string())?;
    rx.recv_timeout(std::time::Duration::from_secs(10))
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn hide_snap_panel(app: tauri::AppHandle) -> Result<(), String> {
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    use tauri::Manager;
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let app_clone = app.clone();
    app.run_on_main_thread(move || {
        if let Some(w) = app_clone.get_webview_window("snap-panel") {
            let mut smaller_frame: Option<NSRect> = None;
            let (pw, ph) = window_snap::panel_dimensions();
            // SAFETY: setIgnoresMouseEvents/frame 均为 NSWindow 标准方法/属性。
            unsafe {
                if let Ok(raw) = w.ns_window() {
                    if let Some(ns) = raw.cast::<objc2_app_kit::NSWindow>().as_ref() {
                        ns.setIgnoresMouseEvents(true);
                        // 动画终点：固定 80% 尺寸（不读 ns.frame() 的 size，避免漂移）；
                        // 中心点取当前 frame（动画为中心对齐缩放，中心点稳定）
                        let cur = ns.frame();
                        let cx = cur.origin.x + cur.size.width / 2.0;
                        let cy = cur.origin.y + cur.size.height / 2.0;
                        let sw = pw * 0.8;
                        let sh = ph * 0.8;
                        smaller_frame = Some(NSRect::new(
                            NSPoint::new(cx - sw / 2.0, cy - sh / 2.0),
                            NSSize::new(sw, sh),
                        ));
                    }
                }
            }
            // alpha + frame 同步动画（目标→缩小 + alpha 1→0，单 group）
            if let Some(sf) = smaller_frame {
                crate::platform::window::animate_panel(
                    &w,
                    0.0,
                    sf.origin.x,
                    sf.origin.y,
                    sf.size.width,
                    sf.size.height,
                    0.25,
                );
            }
        }
        // 用户点击触发的 layout 路径:panel 已 makeKey 偷走 system key,
        // 隐藏后需 deactivate + activate 原 app,把 first responder 还回去。
        #[cfg(target_os = "macos")]
        {
            crate::platform::focus::restore_captured();
        }
        let _ = tx.send(());
    })
    .map_err(|e| e.to_string())?;
    rx.recv_timeout(std::time::Duration::from_secs(10))
        .map_err(|e| e.to_string())?;
    Ok(())
}
