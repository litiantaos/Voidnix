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

/// snap-panel 进出场：淡入淡出 + 轻微纵向位移动画（尺寸固定，无 reflow）。
/// 进：自上滑入 + fade in（easeOut）；出：上移淡出（easeIn）。系统浮层手感。
const SNAP_PANEL_ANIM_SECS: f64 = 0.2;
/// AppKit 坐标 y 向上；正值 = 起点/终点更靠屏幕上方（自顶缘滑入/滑出）。
const SNAP_PANEL_SLIDE_PT: f64 = 10.0;

#[tauri::command]
pub async fn show_snap_panel(app: tauri::AppHandle) -> Result<(), String> {
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    use tauri::Manager;
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let app_clone = app.clone();
    app.run_on_main_thread(move || {
        if let Some(w) = app_clone.get_webview_window("snap-panel") {
            let (pw, ph) = window_snap::panel_dimensions();
            let mut target: Option<NSRect> = None;
            // SAFETY: setFrame/setAlpha/makeKeyAndOrderFront 均为 NSWindow 标准方法。
            // show_panel 已把窗口放到满尺寸目标位；此处只抬高 y 作滑入起点，宽高不变。
            unsafe {
                if let Ok(raw) = w.ns_window() {
                    if let Some(ns) = raw.cast::<objc2_app_kit::NSWindow>().as_ref() {
                        let cur = ns.frame();
                        // 锁定满尺寸（避免前次离场残留半透明小帧）
                        let x = cur.origin.x + cur.size.width / 2.0 - pw / 2.0;
                        let y = cur.origin.y + cur.size.height / 2.0 - ph / 2.0;
                        ns.setFrame_display(
                            NSRect::new(
                                NSPoint::new(x, y + SNAP_PANEL_SLIDE_PT),
                                NSSize::new(pw, ph),
                            ),
                            false,
                        );
                        ns.setAlphaValue(0.0);
                        // NonactivatingPanel：makeKey 不抢前台 app
                        let _: () = objc2::msg_send![
                            ns,
                            makeKeyAndOrderFront: std::ptr::null::<objc2::runtime::AnyObject>()
                        ];
                        target = Some(NSRect::new(NSPoint::new(x, y), NSSize::new(pw, ph)));
                    }
                }
            }
            if let Some(tf) = target {
                crate::platform::window::animate_panel(
                    &w,
                    crate::platform::window::PanelAnimTarget {
                        alpha: 1.0,
                        x: tf.origin.x,
                        y: tf.origin.y,
                        w: tf.size.width,
                        h: tf.size.height,
                        duration: SNAP_PANEL_ANIM_SECS,
                        ease_out: true,
                    },
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
            let (pw, ph) = window_snap::panel_dimensions();
            let mut end: Option<NSRect> = None;
            // SAFETY: setIgnoresMouseEvents / frame 均为 NSWindow 标准 API。
            unsafe {
                if let Ok(raw) = w.ns_window() {
                    if let Some(ns) = raw.cast::<objc2_app_kit::NSWindow>().as_ref() {
                        ns.setIgnoresMouseEvents(true);
                        let cur = ns.frame();
                        let x = cur.origin.x + cur.size.width / 2.0 - pw / 2.0;
                        let y = cur.origin.y + cur.size.height / 2.0 - ph / 2.0;
                        // 终点：上移 10pt + 淡出，宽高仍满尺寸
                        end = Some(NSRect::new(
                            NSPoint::new(x, y + SNAP_PANEL_SLIDE_PT),
                            NSSize::new(pw, ph),
                        ));
                    }
                }
            }
            if let Some(ef) = end {
                crate::platform::window::animate_panel(
                    &w,
                    crate::platform::window::PanelAnimTarget {
                        alpha: 0.0,
                        x: ef.origin.x,
                        y: ef.origin.y,
                        w: ef.size.width,
                        h: ef.size.height,
                        duration: SNAP_PANEL_ANIM_SECS,
                        ease_out: false,
                    },
                );
            }
            // 焦点归还等动画结束，避免中途抢前台
            #[cfg(target_os = "macos")]
            {
                let delay = std::time::Duration::from_secs_f64(SNAP_PANEL_ANIM_SECS + 0.02);
                let app_restore = app_clone.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(delay);
                    let _ = app_restore.run_on_main_thread(|| {
                        if !window_snap::is_panel_visible() {
                            crate::platform::focus::restore_captured();
                        }
                    });
                });
            }
        }
        let _ = tx.send(());
    })
    .map_err(|e| e.to_string())?;
    rx.recv_timeout(std::time::Duration::from_secs(10))
        .map_err(|e| e.to_string())?;
    Ok(())
}
