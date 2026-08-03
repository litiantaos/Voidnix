use super::crop::crop_with_annotation;
use super::ffi::{
    decode_image_data, png_bytes_to_cgimage, voidnix_screenshot_install_background_layer,
    voidnix_screenshot_set_background, voidnix_screenshot_set_background_centered, CGImageRelease,
};

use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Mutex;

// 关闭按钮 28 + 上下/左右各 8 边距 = 44
const PIN_MIN_SIZE: f64 = 44.0;

// pin 窗口关闭时需要恢复焦点的目标 PID
// 由 screenshot exit_impl 写入，pin 创建时读取
pub(super) static PIN_PREV_PID: AtomicI32 = AtomicI32::new(0);

/// pin 窗口临时文件 guard 注册表：label → TempHandle。
/// 窗口创建成功后插入，WindowEvent::Destroyed 时移除（Drop 自动删文件）。
static PIN_TEMPS: std::sync::LazyLock<Mutex<HashMap<String, crate::runtime::storage::TempHandle>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// pin 窗口原始 content 尺寸（创建时记录），滚轮缩放的绝对基准。
/// 不读当前 frame 迭代相乘（NSWindow 可能规整 frame / min 单维钳位破坏比例，误差逐帧锁入基准累积漂移），
/// 改用 orig×scale 一次算出，比例恒等于原图。销毁时同步移除。
static PIN_ORIG_SIZE: std::sync::LazyLock<Mutex<HashMap<String, (f64, f64)>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// sticky center 缩放锚点：缩放序列内沿用首帧窗口中心，不每帧从 frame 重读——
/// setFrame 后系统规整 origin（亚像素/约束纠正）的误差会逐帧锁入基准、向固定方向累积漂移。
#[derive(Clone, Copy)]
struct PinZoomState {
    center: (f64, f64), // 窗口中心，Quartz 屏幕坐标
    last: std::time::Instant,
}
static PIN_ZOOM: std::sync::LazyLock<Mutex<HashMap<String, PinZoomState>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));
/// 缩放序列间隔阈值：距上次缩放超过此值视为新序列，重读 frame 中心
const ZOOM_GAP: std::time::Duration = std::time::Duration::from_millis(250);

#[tauri::command]
pub async fn pin_image(
    app: tauri::AppHandle,
    sel_x: f64,
    sel_y: f64,
    sel_w: f64,
    sel_h: f64,
    scale: f64,
    annotation_png: String,
) -> Result<(), String> {
    let ann = if annotation_png.is_empty() {
        None
    } else {
        Some(decode_image_data(&annotation_png)?)
    };

    #[cfg(target_os = "macos")]
    {
        // 钉图与 exit 并发（前端 fire-and-forget pin + 立刻 doCancel）；入口即快照 origin，
        // 避免 exit 清 CAPTURE_SURFACE 后位置落到 (0,0)。
        let (ox, oy) = super::session::capture_origin();
        // 对齐到整数点（消除亚像素）：前端 sel 来自 webview 亚像素鼠标坐标，
        // 直接传入会让 crop `as isize` 截断丢像素、NSWindow 对亚像素尺寸规整，
        // 二者叠加导致 image 像素 ≠ contentView backing 物理像素，kCAGravityResize
        // 随之纵向拉伸，表现为钉图"略放大 + 内容下移"。整数点下三者严格 1:1。
        let sel_x = sel_x.round();
        let sel_y = sel_y.round();
        let sel_w = sel_w.round();
        let sel_h = sel_h.round();
        let png = crop_with_annotation(sel_x, sel_y, sel_w, sel_h, scale, ann.as_deref())?;
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let path = std::env::temp_dir().join(format!("voidnix_pin_{}.png", ts));
        std::fs::write(&path, &png).map_err(|e| e.to_string())?;
        // TempHandle：窗口创建失败时 Drop 清理；成功后转入 PIN_TEMPS，窗口 destroy 时清理
        let pin_handle = crate::runtime::storage::TempHandle::new(path.clone());

        let cg_addr = png_bytes_to_cgimage(&png) as usize;

        // 截图小于窗口最小尺寸时，窗口保持最小尺寸，原图居中显示；否则窗口贴合截图尺寸。
        // sel 为屏内本地坐标；pin 窗口 position 用 Quartz 全局左上（+ 捕获屏 origin）。
        let centered = sel_w < PIN_MIN_SIZE || sel_h < PIN_MIN_SIZE;
        let win_w = sel_w.max(PIN_MIN_SIZE);
        let win_h = sel_h.max(PIN_MIN_SIZE);
        let win_x = ox + sel_x - (win_w - sel_w) / 2.0;
        let win_y = oy + sel_y - (win_h - sel_h) / 2.0;
        let label = format!("pin-{}", ts);
        let label_key = label.clone();

        let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let app_clone = app.clone();
        app.run_on_main_thread(move || {
            let cg_ptr = cg_addr as *mut std::ffi::c_void;
            let spec = PinWebviewSpec {
                label: &label,
                width: win_w,
                height: win_h,
                pos_x: win_x,
                pos_y: win_y,
                cg_image: cg_ptr,
                centered,
            };
            let r = create_pin_webview(&app_clone, &spec);
            let _ = tx.send(r);
        })
        .map_err(|e| e.to_string())?;
        rx.recv_timeout(std::time::Duration::from_secs(10))
            .map_err(|e| e.to_string())??;

        // 窗口创建成功，转移 handle 到注册表（WindowEvent::Destroyed 时移除 → Drop 删文件）
        if let Ok(mut map) = PIN_TEMPS.lock() {
            map.insert(label_key, pin_handle);
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, ann);
        return Err("仅支持 macOS".to_string());
    }

    Ok(())
}

#[cfg(target_os = "macos")]
struct PinWebviewSpec<'a> {
    label: &'a str,
    width: f64,
    height: f64,
    pos_x: f64,
    pos_y: f64,
    cg_image: *mut std::ffi::c_void,
    centered: bool,
}

#[cfg(target_os = "macos")]
fn create_pin_webview(app: &tauri::AppHandle, spec: &PinWebviewSpec) -> Result<(), String> {
    use objc2_app_kit::{NSWindow, NSWindowCollectionBehavior};
    use tauri::{WebviewUrl, WebviewWindowBuilder};

    let url = WebviewUrl::App("pin.html".into());

    let builder = WebviewWindowBuilder::new(app, spec.label, url)
        .title("")
        .inner_size(spec.width, spec.height)
        .position(spec.pos_x, spec.pos_y)
        .min_inner_size(PIN_MIN_SIZE, PIN_MIN_SIZE)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(true)
        .visible(true)
        .accept_first_mouse(true);

    let window = builder.build().map_err(|e| e.to_string())?;

    // 记录原始 content 尺寸，供滚轮缩放作绝对基准（见 scale_pin_window）
    if let Ok(mut m) = PIN_ORIG_SIZE.lock() {
        m.insert(spec.label.to_string(), (spec.width, spec.height));
    }

    // 窗口销毁时移除 PIN_TEMPS / PIN_ORIG_SIZE 条目（前者 Drop TempHandle → 删临时 PNG）
    let label_for_destroy = spec.label.to_string();
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Destroyed) {
            if let Ok(mut map) = PIN_TEMPS.lock() {
                map.remove(&label_for_destroy);
            }
            if let Ok(mut m) = PIN_ORIG_SIZE.lock() {
                m.remove(&label_for_destroy);
            }
            if let Ok(mut z) = PIN_ZOOM.lock() {
                z.remove(&label_for_destroy);
            }
        }
    });

    // 不调 apply_cached_appearance：NSWindow.setAppearance 在刚 build 出的 WKWebView 上
    // 会触发 prefers-color-scheme 重算死锁主线程（无论窗口 visible 与否）。
    // pin 窗口主题由前端 theme.ts 读 get_cached_appearance 命令拿 main 缓存的强制值，
    // 直接设 DOM data-theme，不依赖原生 setAppearance，主题完全正确且不死锁。

    if let Ok(raw) = window.ns_window().map(|p| p.cast::<NSWindow>()) {
        // SAFETY: ns 经 as_ref Some 分支非空校验；setHidesOnDeactivate:/setLevel:/
        // setCollectionBehavior/setContentSize/contentView/layer/setCornerRadius:/
        // setMasksToBounds:/makeKeyAndOrderFront: 均为 NSWindow/CALayer 标准选择子；
        // cg_image null 检查后传给 FFI（install_background_layer/set_background 自管所有权）
        unsafe {
            if let Some(ns) = raw.as_ref() {
                let _: () = objc2::msg_send![ns, setHidesOnDeactivate: false];
                ns.setLevel(objc2_app_kit::NSStatusWindowLevel);
                let behavior = NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::FullScreenAuxiliary
                    | NSWindowCollectionBehavior::Stationary;
                ns.setCollectionBehavior(behavior);
                // 居中模式下窗口尺寸 ≠ 图片尺寸，无需锁尺寸
                if !spec.centered {
                    // 强制 setContentSize 锁回 spec 尺寸（整数点），保证 contentView backing
                    // 像素 == image 像素，kCAGravityResize 不拉伸。
                    // 不用 setContentAspectRatio——它在每次 setFrame 后的 layout pass 会纠正
                    // 尺寸连带改 origin，致滚轮缩放中心逐帧漂移；比例由 orig×scale 自行保证。
                    ns.setContentSize(objc2_foundation::NSSize::new(spec.width, spec.height));
                }
                if let Some(content_view) = ns.contentView() {
                    let _: () = objc2::msg_send![&content_view, setWantsLayer: true];
                    let layer: *mut objc2::runtime::AnyObject =
                        objc2::msg_send![&content_view, layer];
                    if !layer.is_null() {
                        // 对齐 radius-panel（10）
                        let _: () = objc2::msg_send![layer, setCornerRadius: 10.0_f64];
                        let _: () = objc2::msg_send![layer, setMasksToBounds: true];
                    }
                }

                let ns_window_void = raw.cast::<NSWindow>() as *mut std::ffi::c_void;
                if !spec.cg_image.is_null() {
                    voidnix_screenshot_install_background_layer(ns_window_void);
                    if spec.centered {
                        voidnix_screenshot_set_background_centered(ns_window_void, spec.cg_image);
                    } else {
                        voidnix_screenshot_set_background(ns_window_void, spec.cg_image);
                    }
                    CGImageRelease(spec.cg_image);
                }

                let _: () = objc2::msg_send![
                    ns,
                    makeKeyAndOrderFront: std::ptr::null::<objc2::runtime::AnyObject>()
                ];
            }
        }
    }

    crate::platform::focus::activate_app();

    Ok(())
}

/// 恢复 pin 窗口关闭前的焦点应用。
#[tauri::command]
pub async fn restore_pin_focus(window: tauri::WebviewWindow) {
    let pid = PIN_PREV_PID.load(Ordering::SeqCst);
    if pid > 0 {
        // 先隐藏窗口，确保其不再是 key window
        let _ = window.hide();
        // 再 deactivate 触发系统重新评估 key window，恢复到原应用
        crate::platform::focus::deactivate_app();
        crate::platform::focus::activate_app_by_pid(pid);
        PIN_PREV_PID.store(0, Ordering::SeqCst);
    }
}

#[tauri::command]
pub async fn set_pin_window_opacity(
    window: tauri::WebviewWindow,
    opacity: f64,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::NSWindow;
        use tauri::Manager;
        let app_handle = window.app_handle().clone();
        let opacity_val = opacity.clamp(0.1, 1.0);
        app_handle
            .run_on_main_thread(move || {
                if let Ok(raw) = window.ns_window().map(|p| p.cast::<NSWindow>()) {
                    // SAFETY: ns 经 as_ref Some 分支非空校验；setAlphaValue 为 NSWindow 标准方法
                    unsafe {
                        if let Some(ns) = raw.as_ref() {
                            ns.setAlphaValue(opacity_val);
                        }
                    }
                }
            })
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, opacity);
    }
    Ok(())
}

/// 滚轮缩放：原始尺寸 × 绝对 scale 算目标尺寸，以窗口中心为锚 setFrame:display:（display:false，
/// 图像由 native CALayer 直接渲染，frame 改即重采样，避免 display:true 连续缩放阻塞主线程）。
///
/// sticky center：缩放序列内（连续滚轮）沿用首帧锚定的中心点，不每帧从 frame() 重读——setFrame
/// 后系统规整 origin（亚像素/约束纠正）的误差会逐帧锁入基准、向固定方向（Quartz origin 减小 =
/// 左下）累积漂移。距上次缩放超 ZOOM_GAP 视为新序列（用户停顿后再滚），从 frame 重读中心。
/// 比例由 orig×scale 保证（不读 frame 迭代相乘）；不设 setContentAspectRatio（其 layout pass
/// 连带改 origin 同致漂移）。
#[tauri::command]
pub async fn scale_pin_window(window: tauri::WebviewWindow, scale: f64) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::NSWindow;
        use objc2_foundation::{NSPoint, NSRect, NSSize};
        use std::time::Instant;
        use tauri::Manager;
        let label = window.label().to_string();
        // 取创建时记录的原始尺寸；缺则跳过（理论上不发生）
        let (orig_w, orig_h) = match PIN_ORIG_SIZE.lock() {
            Ok(m) => match m.get(&label).copied() {
                Some(v) => v,
                None => return Ok(()),
            },
            Err(_) => return Ok(()),
        };
        let app_handle = window.app_handle().clone();
        let scale = scale.clamp(0.01, 100.0);
        app_handle
            .run_on_main_thread(move || {
                if let Ok(raw) = window.ns_window().map(|p| p.cast::<NSWindow>()) {
                    // SAFETY: ns 经 as_ref Some 分支非空校验；frame/setFrame:display: 为 NSWindow 标准选择子
                    unsafe {
                        if let Some(ns) = raw.as_ref() {
                            // 读当前 frame 中心（Quartz 坐标系内自洽）
                            let now = Instant::now();
                            let frame = ns.frame();
                            let frame_cx = frame.origin.x + frame.size.width / 2.0;
                            let frame_cy = frame.origin.y + frame.size.height / 2.0;
                            // sticky center：序列内（距上次缩放 ≤ ZOOM_GAP）沿用首帧锚点，不每帧
                            // 重读 frame 中心——setFrame 后系统规整 origin 的误差会累积漂移
                            let (cx, cy) = match PIN_ZOOM.lock() {
                                Ok(mut m) => {
                                    let prev = m.get(&label).copied();
                                    let is_new = match prev {
                                        Some(s) => now.duration_since(s.last) > ZOOM_GAP,
                                        None => true,
                                    };
                                    let c = if is_new {
                                        (frame_cx, frame_cy)
                                    } else {
                                        prev.unwrap().center
                                    };
                                    m.insert(
                                        label.clone(),
                                        PinZoomState {
                                            center: c,
                                            last: now,
                                        },
                                    );
                                    c
                                }
                                Err(_) => (frame_cx, frame_cy),
                            };
                            let mut new_w = orig_w * scale;
                            let mut new_h = orig_h * scale;
                            // 等比下限：任一维 < min 时整体放大到 min，保持比例（不单维钳位）
                            let min_dim = new_w.min(new_h);
                            if min_dim < PIN_MIN_SIZE {
                                let k = PIN_MIN_SIZE / min_dim;
                                new_w *= k;
                                new_h *= k;
                            }
                            let new_rect = NSRect::new(
                                NSPoint::new(cx - new_w / 2.0, cy - new_h / 2.0),
                                NSSize::new(new_w, new_h),
                            );
                            ns.setFrame_display(new_rect, false);
                        }
                    }
                }
            })
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, scale);
    }
    Ok(())
}

/// 查全局鼠标位置（屏幕坐标，左上原点，CSS 像素）。
/// pin 窗口失焦时 WKWebView 不派发 mouseenter/leave，前端 rAF 轮询此值
/// 自行计算 hover 状态。
#[tauri::command]
pub fn pin_global_mouse() -> (f64, f64) {
    #[cfg(target_os = "macos")]
    {
        let mut x = 0.0f64;
        let mut y = 0.0f64;
        // SAFETY: voidnix_screenshot_get_mouse_location 写入 &mut f64（栈变量），
        // screen_height=0.0 表示由 FFI 内部解析屏幕高度；纯输出参数无所有权
        unsafe {
            super::ffi::voidnix_screenshot_get_mouse_location(&mut x, &mut y, 0.0);
        }
        (x, y)
    }
    #[cfg(not(target_os = "macos"))]
    {
        (0.0, 0.0)
    }
}
