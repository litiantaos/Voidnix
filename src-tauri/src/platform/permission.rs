/// 屏幕录制权限：CGPreflightScreenCaptureAccess 只查 TCC 状态，不触发截屏、不分配位图（纳秒级）。
/// 不用 CGDisplayCreateImage——那是真实 GPU 截屏（WindowServer 编码 framebuffer），阻塞数十 ms。
#[cfg(target_os = "macos")]
pub fn check_screen_recording() -> bool {
    extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
    }
    unsafe { CGPreflightScreenCaptureAccess() }
}

#[cfg(not(target_os = "macos"))]
pub fn check_screen_recording() -> bool {
    false
}

/// 应用 bundle 根路径（`…/Voidnix.app`）；dev 裸二进制无 .app 祖先返回 None。
#[cfg(target_os = "macos")]
pub fn app_bundle_path() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.ancestors()
        .nth(3)
        .filter(|p| p.extension().is_some_and(|e| e == "app"))
        .map(|p| p.to_path_buf())
}

/// 应用公证状态：stapler validate 本地校验 stapled ticket（发布链公证后必 staple，
/// 检测等价），一次性缓存。macOS 15+ 未公证应用（Apple Development / adhoc 签名）的
/// 辅助功能/屏幕录制 API 请求路径写入的 TCC 条目无效（开关打开也不生效），
/// 授权须走系统设置手动添加；授权入口按此分流，公证后自动切回 API 请求路径。
#[cfg(target_os = "macos")]
pub fn check_app_notarized() -> bool {
    static CACHED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *CACHED.get_or_init(|| {
        let target =
            app_bundle_path().unwrap_or_else(|| std::env::current_exe().unwrap_or_default());
        std::process::Command::new("xcrun")
            .args(["stapler", "validate"])
            .arg(&target)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
}

#[cfg(not(target_os = "macos"))]
pub fn check_app_notarized() -> bool {
    false
}

/// 屏幕录制权限请求：CGRequestScreenCaptureAccess 弹系统授权对话框，并把本应用
/// 注册进系统设置「屏幕录制」列表——macOS 15+ 未主动请求的应用不出现在列表中，
/// 用户只能从访达手动添加。同步阻塞至用户处理弹窗；命令层 async + spawn_blocking
/// （同步命令在主线程执行）。授权后系统自行引导退出重开（预绑定权限需重启生效）。
#[cfg(target_os = "macos")]
pub fn request_screen_recording() -> bool {
    extern "C" {
        fn CGRequestScreenCaptureAccess() -> bool;
    }
    unsafe { CGRequestScreenCaptureAccess() }
}

#[cfg(not(target_os = "macos"))]
pub fn request_screen_recording() -> bool {
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

/// 全磁盘访问权限检查：open 系统级 TCC 数据库（仅 FDA 可读，静默 EPERM）。
/// 不试读 ~/Desktop——桌面/下载等目录属「文件与文件夹」TCC 管辖，首次触碰会触发
/// 系统询问弹窗，且用户单独授权「桌面文件夹」时会误报 FDA。
/// open 系统文件不触发任何弹窗，微秒级。
#[cfg(target_os = "macos")]
pub fn check_full_disk_access() -> bool {
    std::fs::File::open("/Library/Application Support/com.apple.TCC/TCC.db").is_ok()
}

#[cfg(not(target_os = "macos"))]
pub fn check_full_disk_access() -> bool {
    false
}

/// 打开系统设置中对应隐私面板并启动授权会话（时序详见 spawn_perm_session，每步
/// 原语均经探针实测）。kind: "accessibility" | "screen_recording" | "full_disk_access"
#[cfg(target_os = "macos")]
pub fn open_privacy_settings(app: &tauri::AppHandle, kind: &str) {
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
                .status();
            return;
        }
    };
    spawn_perm_session(app, kind, url);
    // 会话起点事件：Rust 直发的会话（finder-ext 辅助功能引导）没有前端入口置钉，
    // 前端经此统一置 permGrantKind，与 startPermGrant 共享钉住/linger 链路
    {
        use tauri::Emitter;
        let _ = app.emit("perm-session", kind);
    }
}

#[cfg(not(target_os = "macos"))]
pub fn open_privacy_settings(_app: &tauri::AppHandle, _kind: &str) {}

// ── 授权会话 ──

/// 会话代数：每次发起递增，旧会话线程检测到不匹配即退出（perm-flow 由最新会话发出，
/// 前端钉住语义跨会话连续）。
#[cfg(target_os = "macos")]
static PERM_SESSION_GEN: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// `perm-flow` 事件载荷。
#[derive(Clone, serde::Serialize)]
#[cfg(target_os = "macos")]
struct PermFlow<'a> {
    kind: &'a str,
    granted: bool,
}

#[cfg(target_os = "macos")]
fn spawn_perm_session(app: &tauri::AppHandle, kind: &str, url: &str) {
    use std::time::{Duration, Instant};

    let gen = PERM_SESSION_GEN.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    let app = app.clone();
    let kind = kind.to_string();
    let url = url.to_string();
    std::thread::spawn(move || {
        use std::process::Command;
        let stale = || PERM_SESSION_GEN.load(std::sync::atomic::Ordering::SeqCst) != gen;

        // 1) 主窗先降层置顶（普通层）：后续设置窗口映射/抬升自然落其上
        demote_main_window(&app);
        // 2) 打开设置面板（LaunchServices 激活设置）
        let _ = Command::new("open").arg(&url).status();

        // 3) 设置应用就绪即激活（冷启动需 1-2s）
        for _ in 0..20 {
            if activate_settings_app() {
                break;
            }
            if stale() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        // 4) 设置窗口映射后主窗避让（冷启动窗口晚于进程就绪，等足 4s）
        let mut dodged = false;
        for _ in 0..40 {
            if stale() {
                return;
            }
            if dodge_beside_settings(&app) {
                dodged = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        // 5) open 重发置顶：demote 的 orderFront 把主窗压到普通层顶，设置已激活时
        //    activate 为空操作，LaunchServices 重发恒能把设置窗抬到主窗之上
        //    （同 URL 仅重复导航，无副作用）。换代即放弃：旧会话重发会把设置
        //    导航回旧面板，与最新会话轮询的权限项错位
        if dodged {
            if stale() {
                return;
            }
            let _ = Command::new("open").arg(&url).status();
        }
        // 6) 轮询授权状态；完全访问/录屏并行跟踪重启确认弹窗生命周期。这两项授权
        //    须重启才对运行中进程生效——check 恒 false，授权检测不会命中，以弹窗
        //    取消为会话终点（收浮窗 + 补聚焦）；弹窗形态双路检测：设置自持窗
        //    差分 / 跨进程前台切换
        let deadline = Instant::now() + Duration::from_secs(600);
        let watch_dialog = kind != "accessibility";
        let our_pid = std::process::id() as i32;
        let settings_pid = if watch_dialog {
            settings_app().map(|a| a.processIdentifier())
        } else {
            None
        };
        let baseline_ids: Vec<i64> = settings_pid
            .and_then(settings_windows)
            .map(|w| w.iter().map(|(n, _)| *n).collect())
            .unwrap_or_default();
        let mut watch_win: Option<i64> = None;
        let mut watch_foreign = false;
        let mut sys_streak = 0u32;
        // 授权检测已收尾（live 生效命中，弹窗可能仍在路上）：后续循环仅跟踪弹窗
        // 取消，届时补聚焦即止、不再二次收尾
        let mut granted_done = false;
        loop {
            std::thread::sleep(Duration::from_secs(1));
            if stale() {
                return;
            }
            if !granted_done {
                let granted = match kind.as_str() {
                    "accessibility" => check_accessibility(),
                    "screen_recording" => check_screen_recording(),
                    _ => check_full_disk_access(),
                };
                if granted {
                    granted_done = true;
                    // 设备控制即取键盘焦点收尾；完全访问/录屏 live 命中（重授权等
                    // 场景）弹窗可能仍在路上——不夺 key（否则弹窗降为非激活、双击
                    // 才点到按钮），继续跟踪其取消后补聚焦
                    finish_perm_session(&app, &kind, true, kind == "accessibility");
                    if kind == "accessibility" {
                        return;
                    }
                } else if Instant::now() > deadline {
                    finish_perm_session(&app, &kind, false, false);
                    return;
                }
            }
            // 弹窗跟踪：取消即收尾（granted 取当前检测结果——FDA/录屏重启前恒
            // false，已授权状态待应用重启后 check 命中呈现）
            if let (true, Some(pid)) = (watch_dialog, settings_pid) {
                let cancelled = if let Some(dw) = watch_win {
                    settings_windows(pid).is_some_and(|w| !w.iter().any(|(n, _)| *n == dw))
                } else if watch_foreign {
                    crate::platform::focus::current_frontmost_pid() == Some(pid)
                } else if let Some(wins) = settings_windows(pid) {
                    if let Some((n, _)) = wins
                        .iter()
                        .find(|(n, l)| *l != 0 || !baseline_ids.contains(n))
                    {
                        // 非零层级兜底弹窗早于基线出现的场景（主窗恒 layer 0）
                        watch_win = Some(*n);
                        false
                    } else {
                        let fm = crate::platform::focus::current_frontmost_pid();
                        let sys_dialog =
                            fm.is_some_and(|p| p != pid && p != our_pid && is_system_process(p));
                        sys_streak = if sys_dialog { sys_streak + 1 } else { 0 };
                        if sys_streak >= 2 {
                            watch_foreign = true;
                        }
                        false
                    }
                } else {
                    false
                };
                if cancelled {
                    if granted_done {
                        make_main_key(&app);
                    } else {
                        finish_perm_session(&app, &kind, false, true);
                    }
                    return;
                }
            }
        }
    });
}

/// 会话收尾：emit `perm-flow` + 主窗复位（make_key 时 panel makeKey 取键盘焦点）。
#[cfg(target_os = "macos")]
fn finish_perm_session(app: &tauri::AppHandle, kind: &str, granted: bool, make_key: bool) {
    use tauri::Emitter;
    let _ = app.emit("perm-flow", PermFlow { kind, granted });
    let restore_app = app.clone();
    let _ = app.run_on_main_thread(move || {
        use tauri::Manager;
        if let Some(win) = restore_app.get_webview_window("main") {
            crate::platform::window::restore_window_order(&win, make_key);
        }
    });
}

/// 系统设置运行实例。bundle id 跨版本漂移：Ventura 起为 com.apple.systemsettings，
/// 实测 macOS 27 的 /System/Applications/System Settings.app 回落
/// com.apple.systempreferences——按序探测取首个命中，勿假设单一 id。
#[cfg(target_os = "macos")]
fn settings_app() -> Option<objc2::rc::Retained<objc2_app_kit::NSRunningApplication>> {
    use objc2_app_kit::NSRunningApplication;
    use objc2_foundation::NSString;
    ["com.apple.systemsettings", "com.apple.systempreferences"]
        .iter()
        .find_map(|id| {
            NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(id))
                .firstObject()
        })
}

/// 激活置顶系统设置。NSRunningApplication 线程安全，会话线程直调。
/// macOS 14+ ActivateIgnoringOtherApps 已废弃且无效果，空选项 activate 即可置顶。
#[cfg(target_os = "macos")]
fn activate_settings_app() -> bool {
    use objc2_app_kit::NSApplicationActivationOptions;
    let Some(app) = settings_app() else {
        return false;
    };
    app.activateWithOptions(NSApplicationActivationOptions::empty())
}

/// 主窗 makeKey（panel 语义不激活 NSApp，主线程）。
#[cfg(target_os = "macos")]
fn make_main_key(app: &tauri::AppHandle) {
    let key_app = app.clone();
    let _ = app.run_on_main_thread(move || {
        use tauri::Manager;
        if let Some(win) = key_app.get_webview_window("main") {
            crate::platform::window::make_key_window(&win);
        }
    });
}

/// 设置进程当前 on-screen 窗口清单（窗口号 + 层级）。任意层级任意尺寸——sheet/
/// 模态弹窗均以独立窗口呈现于窗口列表，与 settings_window_frame 的主窗探测过滤
/// 条件不同；CGWindowList 线程安全，会话线程直调。
#[cfg(target_os = "macos")]
fn settings_windows(pid: i32) -> Option<Vec<(i64, i64)>> {
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    let array = crate::platform::window_list::copy_on_screen_windows()?;
    let key_pid = CFString::from_static_string("kCGWindowOwnerPID");
    let key_num = CFString::from_static_string("kCGWindowNumber");
    let key_layer = CFString::from_static_string("kCGWindowLayer");
    let mut out = Vec::new();
    for i in 0..array.len() {
        let Some(dict) = array.get(i) else { continue };
        let owner = crate::platform::window_list::dict_lookup(&dict, &key_pid)
            .and_then(|v| v.downcast::<CFNumber>())
            .and_then(|n| n.to_i64())
            .unwrap_or(0);
        if owner != pid as i64 {
            continue;
        }
        let num = crate::platform::window_list::dict_lookup(&dict, &key_num)
            .and_then(|v| v.downcast::<CFNumber>())
            .and_then(|n| n.to_i64())
            .unwrap_or(0);
        let layer = crate::platform::window_list::dict_lookup(&dict, &key_layer)
            .and_then(|v| v.downcast::<CFNumber>())
            .and_then(|n| n.to_i64())
            .unwrap_or(0);
        out.push((num, layer));
    }
    Some(out)
}

/// 进程可执行文件是否系统路径（/System/ 或 /usr/libexec）——重启确认弹窗由系统
/// 进程承载。proc_pidpath 免权限读取。
#[cfg(target_os = "macos")]
fn is_system_process(pid: i32) -> bool {
    use std::ffi::{c_int, c_void, CStr};
    extern "C" {
        // 与 mem.rs 声明同签名（重复 extern 需一致，否则 clashing_extern_declarations）
        fn proc_pidpath(pid: c_int, buffer: *mut c_void, buffersize: u32) -> c_int;
    }
    let mut buf = [0 as std::ffi::c_char; 4096];
    let n = unsafe { proc_pidpath(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
    if n <= 0 {
        return false;
    }
    let path = unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy();
    path.starts_with("/System/") || path.starts_with("/usr/libexec")
}

/// 主窗降层置顶（AppKit 须主线程，经 channel 同步等待）。
#[cfg(target_os = "macos")]
fn demote_main_window(app: &tauri::AppHandle) {
    use tauri::Manager;
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let app_for_main = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(win) = app_for_main.get_webview_window("main") {
            crate::platform::window::demote_window_level(&win);
        }
        let _ = tx.send(());
    });
    let _ = rx.recv_timeout(std::time::Duration::from_secs(2));
}

/// 主窗避让到系统设置窗口旁。AppKit 部分须主线程执行，经 channel 同步等待结果。
#[cfg(target_os = "macos")]
fn dodge_beside_settings(app: &tauri::AppHandle) -> bool {
    let (tx, rx) = std::sync::mpsc::channel::<bool>();
    let app_for_main = app.clone();
    if app
        .run_on_main_thread(move || {
            let _ = tx.send(dodge_on_main(&app_for_main));
        })
        .is_err()
    {
        return false;
    }
    rx.recv_timeout(std::time::Duration::from_secs(2))
        .unwrap_or(false)
}

/// 系统设置主窗 frame（Cocoa，仅主线程）：CGWindowList 按设置进程 pid 找 layer-0
/// 最大窗（bounds 无需屏幕录制权限、layer-0 排除悬浮 chrome），Quartz（左上原点
/// y 向下）经主屏 frame 高翻转为 Cocoa（左下原点 y 向上）。供授权会话避让与
/// 拖拽指引浮窗定位共用。
#[cfg(target_os = "macos")]
pub(crate) fn settings_window_frame(
    mtm: objc2_foundation::MainThreadMarker,
) -> Option<objc2_foundation::NSRect> {
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use objc2_foundation::{NSPoint, NSSize};
    use std::ffi::c_void;

    let settings_pid = settings_app().map(|a| a.processIdentifier() as i64)?;
    let array = crate::platform::window_list::copy_on_screen_windows()?;
    let key_layer = CFString::from_static_string("kCGWindowLayer");
    let key_pid = CFString::from_static_string("kCGWindowOwnerPID");
    let key_bounds = CFString::from_static_string("kCGWindowBounds");
    let key_x = CFString::from_static_string("X");
    let key_y = CFString::from_static_string("Y");
    let key_w = CFString::from_static_string("Width");
    let key_h = CFString::from_static_string("Height");
    let mut best: Option<(f64, f64, f64, f64)> = None;
    let mut best_area = 0.0f64;
    for i in 0..array.len() {
        let Some(dict) = array.get(i) else { continue };
        let layer = crate::platform::window_list::dict_lookup(&dict, &key_layer)
            .and_then(|v| v.downcast::<CFNumber>())
            .and_then(|n| n.to_i64())
            .unwrap_or(-1);
        if layer != 0 {
            continue;
        }
        let owner = crate::platform::window_list::dict_lookup(&dict, &key_pid)
            .and_then(|v| v.downcast::<CFNumber>())
            .and_then(|n| n.to_i64())
            .unwrap_or(0);
        if owner != settings_pid {
            continue;
        }
        let Some(bd) = crate::platform::window_list::dict_lookup(&dict, &key_bounds)
            .and_then(|v| v.downcast::<CFDictionary<*const c_void, *const c_void>>())
        else {
            continue;
        };
        let gn = |k: &CFString| -> f64 {
            crate::platform::window_list::dict_lookup(&bd, k)
                .and_then(|v| v.downcast::<CFNumber>())
                .and_then(|n| n.to_f64())
                .unwrap_or(0.0)
        };
        let (x, y, w, h) = (gn(&key_x), gn(&key_y), gn(&key_w), gn(&key_h));
        if w < 200.0 || h < 200.0 {
            continue;
        }
        if w * h > best_area {
            best_area = w * h;
            best = Some((x, y, w, h));
        }
    }
    let (qx, qy, qw, qh) = best?;
    // CG 全局坐标原点是主屏（screens[0]）左上角；mainScreen 是焦点屏（会话期间即
    // 设置所在屏），多屏异高时两者高度不等——翻转必须用主屏高
    let primary_frame = objc2_app_kit::NSScreen::screens(mtm).firstObject()?.frame();
    let primary_h = primary_frame.size.height;
    Some(objc2_foundation::NSRect::new(
        NSPoint::new(qx, primary_h - qy - qh),
        NSSize::new(qw, qh),
    ))
}

/// 主窗避让到设置窗口旁（设置窗经 settings_window_frame 求得）：
/// 两侧放不下（如设置窗近全宽的笔记本屏）时退化为与设置窗重叠最小的边缘位置，
/// 部分遮挡不影响可读（设置恒在主窗之上，见会话时序）。仅定位，不动层级。仅主线程调用。
#[cfg(target_os = "macos")]
fn dodge_on_main(app: &tauri::AppHandle) -> bool {
    use objc2_app_kit::NSWindow;
    use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize};
    use tauri::Manager;

    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    let Some(win) = app.get_webview_window("main") else {
        return false;
    };
    let Some(settings) = settings_window_frame(mtm) else {
        return false;
    };

    let Ok(ptr) = win.ns_window() else {
        return false;
    };
    let raw = ptr.cast::<NSWindow>();
    let Some(ns_window) = (unsafe { raw.as_ref() }) else {
        return false;
    };
    let cur = ns_window.frame();

    let cx = settings.origin.x + settings.size.width / 2.0;
    let cy = settings.origin.y + settings.size.height / 2.0;
    let Some(vis) = crate::platform::window::screen_vis_containing(mtm, cx, cy) else {
        return false;
    };

    // 位置候选（Cocoa 坐标）：并排两侧完全可见优先；贴屏缘/上下条为小屏兜底，
    // 以「与设置窗重叠面积最小」择优（并排候选重叠为 0 恒最优，顺序即优先级）。
    // 层级与激活由会话时序承担（demote_main_window + open 重发），此处仅定位
    const GAP: f64 = 24.0;
    let (w, h) = (cur.size.width, cur.size.height);
    let s_min_x = settings.origin.x;
    let s_max_x = settings.origin.x + settings.size.width;
    let s_min_y = settings.origin.y;
    let s_max_y = settings.origin.y + settings.size.height;
    let s_mid_x = s_min_x + settings.size.width / 2.0;
    let s_mid_y = s_min_y + settings.size.height / 2.0;
    let vis_max_x = vis.origin.x + vis.size.width;
    let vis_max_y = vis.origin.y + vis.size.height;
    let vis_mid_y = vis.origin.y + vis.size.height / 2.0;
    let candidates = [
        (s_max_x + GAP, s_mid_y - h / 2.0),
        (s_min_x - GAP - w, s_mid_y - h / 2.0),
        (vis_max_x - w, vis_mid_y - h / 2.0),
        (vis.origin.x, vis_mid_y - h / 2.0),
        (s_mid_x - w / 2.0, vis.origin.y),
        (s_mid_x - w / 2.0, vis_max_y - h),
    ];
    let overlap = |a: NSRect| -> f64 {
        let ox = (a.origin.x + a.size.width).min(s_max_x) - a.origin.x.max(s_min_x);
        let oy = (a.origin.y + a.size.height).min(s_max_y) - a.origin.y.max(s_min_y);
        if ox > 0.0 && oy > 0.0 {
            ox * oy
        } else {
            0.0
        }
    };
    let mut best_pos: Option<(f64, f64, f64)> = None;
    for (x, y) in candidates {
        let x = x.clamp(vis.origin.x, (vis_max_x - w).max(vis.origin.x));
        let y = y.clamp(vis.origin.y, (vis_max_y - h).max(vis.origin.y));
        let area = overlap(NSRect::new(NSPoint::new(x, y), NSSize::new(w, h)));
        if best_pos.is_none_or(|(_, _, a)| area < a) {
            best_pos = Some((x, y, area));
        }
    }
    if let Some((x, y, _)) = best_pos {
        crate::platform::window::move_main_to(
            &win,
            NSRect::new(NSPoint::new(x, y), NSSize::new(w, h)),
        );
    }
    true
}
