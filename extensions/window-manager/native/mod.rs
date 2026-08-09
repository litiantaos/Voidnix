use crate::runtime::registry::Extension;
use serde::{Deserialize, Serialize};

mod window_snap;

/// 自定义尺寸 floor/cap（权威源，TS config.ts BOUNDS 须手动同步，
/// check:wm-bounds CI 强制约束）。
const WIDTH_BOUNDS: (f64, f64) = (200.0, 4096.0);
const HEIGHT_BOUNDS: (f64, f64) = (200.0, 4096.0);

/// Window manager 扩展。
pub struct WindowManagerExtension;

#[async_trait::async_trait]
impl Extension for WindowManagerExtension {
    fn id(&self) -> &'static str {
        "window-manager"
    }

    // snap-panel 窗口创建 deferred 到 bootstrap 后（spawn_blocking 不阻塞 join_all）。
    // 读 config.json 判断 enabled：仅启用时创建（省一个常驻 WebContent 进程 ~30-50MB）。
    // 不能在 set_window_manager_enabled 的 run_on_main_thread 闭包内创建——
    // WebviewWindowBuilder::build() 内部会 dispatch 到主线程，与正在执行闭包的主线程死锁。
    async fn setup(&self, app: &tauri::AppHandle) -> tauri::Result<()> {
        let app_clone = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            // 读 config.json：未启用则跳过创建
            let enabled = crate::runtime::storage::ext_data_dir(&app_clone, "window-manager")
                .ok()
                .and_then(|d| std::fs::read_to_string(d.join("config.json")).ok())
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                .and_then(|v| v.get("enabled").and_then(|b| b.as_bool()))
                .unwrap_or(false);
            if !enabled {
                return;
            }
            let (tx, rx) = std::sync::mpsc::channel();
            let app2 = app_clone.clone();
            let _ = app_clone.run_on_main_thread(move || {
                create_snap_panel(&app2);
                let _ = tx.send(());
            });
            let _ = rx.recv_timeout(std::time::Duration::from_secs(10));
        });
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
    // 强制主题在 snap-panel 也生效（读 set_window_appearance 缓存）
    crate::platform::window::apply_cached_appearance(&window);
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

/// 按需创建 snap-panel 窗口（start 时调用）。已存在则跳过。
#[cfg(target_os = "macos")]
pub(crate) fn create_snap_panel(app: &tauri::AppHandle) {
    use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
    if app.get_webview_window("snap-panel").is_some() {
        return;
    }
    let url = WebviewUrl::App("snap-panel.html".into());
    if WebviewWindowBuilder::new(app, "snap-panel", url)
        .title("")
        .inner_size(600.0, 300.0)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .build()
        .is_ok()
    {
        configure_snap_panel(app);
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn create_snap_panel(_app: &tauri::AppHandle) {}

/// 单屏几何。
///
/// - `x/y/width/height`：Cocoa `NSScreen.frame`（左下原点、y 向上）—— snap 面板定位 / 鼠标命中
/// - `layout_*`：AX 坐标系下的**布局目标区**（菜单栏 / 真 Dock 侧边与底边 inset；见 `layout_cocoa_rect`）
/// - `ax_frame_*`：AX 坐标系下的全帧 —— 用 AX 窗口坐标判定归属屏
///
/// 坐标翻转锚点是 **primary**（frame 原点 (0,0) 的菜单栏屏），**不是** `NSScreen.mainScreen`
///（后者随焦点漂移，副屏操作时会整屏错位）。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ScreenInfo {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub is_main: bool,
    pub layout_x: f64,
    pub layout_y: f64,
    pub layout_width: f64,
    pub layout_height: f64,
    pub ax_frame_x: f64,
    pub ax_frame_y: f64,
    pub ax_frame_width: f64,
    pub ax_frame_height: f64,
}

impl ScreenInfo {
    /// 无屏 / 非主线程兜底（主屏 1440×900，Cocoa 与 AX 原点重合时数值相同）。
    pub fn fallback() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 1440.0,
            height: 900.0,
            is_main: true,
            layout_x: 0.0,
            layout_y: 0.0,
            layout_width: 1440.0,
            layout_height: 900.0,
            ax_frame_x: 0.0,
            ax_frame_y: 0.0,
            ax_frame_width: 1440.0,
            ax_frame_height: 900.0,
        }
    }

    /// Cocoa rect → AX rect。
    /// `primary_max_y` = primary.frame.origin.y + primary.frame.size.height
    ///（primary = 菜单栏屏 / frame 原点近 (0,0)，非 mainScreen）。
    /// AX_y = primary_max_y − cocoa_y − h（原点：primary 左上，y 向下）。
    pub fn cocoa_rect_to_ax(
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        primary_max_y: f64,
    ) -> (f64, f64, f64, f64) {
        (x, primary_max_y - y - h, w, h)
    }

    /// 布局用 Cocoa 矩形：保留菜单栏顶 inset 与侧/底 **真 Dock** inset；
    /// 抹掉非 Dock 屏上被系统误扣的底边（副屏底部留白主因）。
    ///
    /// `frame` / `visible`：`(x, y, w, h)` Cocoa。
    /// `owns_bottom_dock`：全局仅一屏为底 Dock 宿主（bottom_inset 最大，并列 primary 优先）。
    pub fn layout_cocoa_rect(
        frame: (f64, f64, f64, f64),
        visible: (f64, f64, f64, f64),
        owns_bottom_dock: bool,
    ) -> (f64, f64, f64, f64) {
        let (frame_x, frame_y, frame_w, frame_h) = frame;
        let (vis_x, vis_y, vis_w, vis_h) = visible;
        let frame_top = frame_y + frame_h;
        let frame_right = frame_x + frame_w;
        let bottom_inset = (vis_y - frame_y).max(0.0);
        let (ly, lh) = if bottom_inset > 1.0 && !owns_bottom_dock {
            // 误扣 Dock 高：底回到 frame 底，顶仍用 visible 顶（菜单栏），且不超过 frame 顶
            let top = (vis_y + vis_h).min(frame_top);
            (frame_y, (top - frame_y).max(0.0))
        } else {
            let ly = vis_y.clamp(frame_y, frame_top);
            let top = (vis_y + vis_h).min(frame_top).max(ly);
            (ly, top - ly)
        };
        // 左右仍跟 visible（侧边 Dock / 安全区），夹进 frame
        let lx = vis_x.clamp(frame_x, frame_right);
        let lw = vis_w.min(frame_right - lx).max(0.0);
        (lx, ly, lw, lh)
    }
}

#[cfg(test)]
mod tests {
    use super::ScreenInfo;

    #[test]
    fn cocoa_to_ax_main_screen_top_left() {
        // 主屏 1440×900，Cocoa 全帧 (0,0,1440,900) → AX (0,0,1440,900)
        let (x, y, w, h) = ScreenInfo::cocoa_rect_to_ax(0.0, 0.0, 1440.0, 900.0, 900.0);
        assert_eq!((x, y, w, h), (0.0, 0.0, 1440.0, 900.0));
    }

    #[test]
    fn cocoa_to_ax_secondary_right_bottom_aligned() {
        // 副屏在右侧、底对齐：Cocoa (1440, 0, 1920, 1080)，primary_max_y 900
        // AX 顶边 = 900 - 0 - 1080 = -180
        let (x, y, w, h) = ScreenInfo::cocoa_rect_to_ax(1440.0, 0.0, 1920.0, 1080.0, 900.0);
        assert_eq!((x, y, w, h), (1440.0, -180.0, 1920.0, 1080.0));
    }

    #[test]
    fn cocoa_to_ax_visible_below_menu_bar() {
        // visibleFrame 扣掉菜单栏 25pt：Cocoa y=0 高 875 → AX y=25
        let (x, y, w, h) = ScreenInfo::cocoa_rect_to_ax(0.0, 0.0, 1440.0, 875.0, 900.0);
        assert_eq!((x, y, w, h), (0.0, 25.0, 1440.0, 875.0));
    }

    #[test]
    fn layout_strips_false_dock_inset_on_secondary() {
        // 副屏 frame 与「可见」同底应对齐，但系统误把主屏 Dock 70pt 扣到副屏 visible.y
        let frame = (1920.0, 0.0, 2560.0, 1440.0);
        let vis_false = (1920.0, 70.0, 2560.0, 1370.0); // 底 +70、高 -70，顶仍 1440
        let (x, y, w, h) = ScreenInfo::layout_cocoa_rect(frame, vis_false, false);
        assert_eq!((x, y, w, h), (1920.0, 0.0, 2560.0, 1440.0));
    }

    #[test]
    fn layout_keeps_real_dock_inset_on_owner() {
        let frame = (0.0, 0.0, 1440.0, 900.0);
        let vis = (0.0, 70.0, 1440.0, 805.0); // 底 Dock 70 + 顶菜单 25
        let (x, y, w, h) = ScreenInfo::layout_cocoa_rect(frame, vis, true);
        assert_eq!((x, y, w, h), (0.0, 70.0, 1440.0, 805.0));
    }

    #[test]
    fn cocoa_to_ax_secondary_above_primary() {
        // 副屏叠在主屏上方：主 1800×1169，副 Cocoa (0, 1169, 1920, 1080)
        // AX 顶 = 1169 - 1169 - 1080 = -1080，底边 = 0（贴主屏顶，不越界）
        let (x, y, w, h) = ScreenInfo::cocoa_rect_to_ax(0.0, 1169.0, 1920.0, 1080.0, 1169.0);
        assert_eq!((x, y, w, h), (0.0, -1080.0, 1920.0, 1080.0));
        assert_eq!(y + h, 0.0);
    }

    #[test]
    fn bottom_half_on_secondary_above_stays_within_frame() {
        // 竖排副屏下半：AX layout (-1080,h=1080) → bottom 起点 -540、高 540、底边 0
        let primary_max_y = 1169.0;
        let frame = (0.0, 1169.0, 1920.0, 1080.0);
        let vis = frame;
        let (cx, cy, cw, ch) = ScreenInfo::layout_cocoa_rect(frame, vis, false);
        let (lx, ly, lw, lh) = ScreenInfo::cocoa_rect_to_ax(cx, cy, cw, ch, primary_max_y);
        let hh = lh / 2.0;
        let bottom_y = ly + hh;
        let bottom_h = hh;
        assert_eq!((lx, ly, lw, lh), (0.0, -1080.0, 1920.0, 1080.0));
        assert_eq!(bottom_y, -540.0);
        assert_eq!(bottom_h, 540.0);
        assert_eq!(bottom_y + bottom_h, 0.0); // 不越过副屏底 / 主屏顶
    }

    #[test]
    fn layout_cocoa_rect_side_dock_does_not_overflow_frame() {
        // 左侧 Dock：visible x 抬高；lw 不得超过 frame 右缘
        let frame = (0.0, 0.0, 1440.0, 900.0);
        let vis = (70.0, 0.0, 1370.0, 875.0);
        let (x, y, w, h) = ScreenInfo::layout_cocoa_rect(frame, vis, true);
        assert_eq!((x, y, w, h), (70.0, 0.0, 1370.0, 875.0));
        assert!(x + w <= 1440.0 + f64::EPSILON);
    }
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
    // create_snap_panel 必须在主线程执行：configure_snap_panel → apply_mica_material
    // 内 MainThreadMarker::new().expect() 在非主线程 panic。不能在 run_on_main_thread
    // 闭包外调用（本命令跑在 tokio worker）。移入闭包与 do_set_window_manager_enabled
    // 同批执行（create 幂等，已存在直接 return）。
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let app_clone = app.clone();
    app.run_on_main_thread(move || {
        if enabled {
            create_snap_panel(&app_clone);
        }
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
