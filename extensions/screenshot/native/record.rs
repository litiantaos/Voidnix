//! 录屏：截屏 overlay 工具栏的「录屏」按钮负责选区确认，采集与落盘走
//! ScreenCaptureKit（`record.mm`），结束后经事件驱动主窗（toast + 剪辑台交接）。
//! 无独立录屏快捷键：录制中再按截屏快捷键或点菜单栏「停止录屏」结束。
//!
//! 状态机：Idle → Starting → Recording → Stopping → Idle。所有终态收束都汇到
//! `finish()`（ObjC delegate / 看门狗 / 启动失败），幂等（CAS 只有一次生效）。
//!
//! 为什么录屏归属 screenshot 扩展：选区/权限/overlay 生命周期全在截屏域，录屏只是
//! 「把确认的选区从抓一帧换成录一段」；剪辑台交接走跨扩展事件（见 index.ts），
//! 不在 Rust 侧直呼 video 扩展。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};

use crate::runtime::lock_or_recover;
use crate::runtime::menubar::{MenuBarContribution, MenuEntry};

#[cfg(target_os = "macos")]
extern "C" {
    fn voidnix_record_set_finish_callback(
        callback: extern "C" fn(ok: i32, duration_ms: i64, error: *const std::os::raw::c_char),
    );
    fn voidnix_record_start(
        display_id: u32,
        sel_x: f64,
        sel_y: f64,
        sel_w: f64,
        sel_h: f64,
        scale: f64,
        fps: i32,
        system_audio: bool,
        microphone: bool,
        path: *const std::os::raw::c_char,
        error_buffer: *mut std::os::raw::c_char,
        error_buffer_len: usize,
    ) -> bool;
    fn voidnix_record_stop();
    fn voidnix_record_abort();
}

const PHASE_IDLE: u8 = 0;
const PHASE_STARTING: u8 = 1;
const PHASE_RECORDING: u8 = 2;
const PHASE_STOPPING: u8 = 3;
/// 启动在途时被取消（overlay 关闭 / 录屏快捷键再按）：native start 返回后由
/// 启动命令 abort 原生实例并删除半成品，不会产生后台录制。
const PHASE_CANCELLING: u8 = 4;

static PHASE: AtomicU8 = AtomicU8::new(PHASE_IDLE);
/// 菜单计时线程代次：每次开录自增，旧线程在下一次循环自然退出（防快速
/// 停止/重开时多个 ticker 并存、每秒重复重建菜单）。
static TICKER_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static APP: OnceLock<AppHandle> = OnceLock::new();
static SESSION: Mutex<Option<RecordSession>> = Mutex::new(None);

struct RecordSession {
    path: PathBuf,
    started: Instant,
}

/// 录屏配置（前端 main 窗口 config watch 同步；子窗口无 store 权限，故权威留在 Rust）。
struct RecordOptions {
    save_path: String,
    fps: u32,
    system_audio: bool,
    microphone: bool,
    open_clip: bool,
}

static OPTIONS: Mutex<RecordOptions> = Mutex::new(RecordOptions {
    save_path: String::new(),
    fps: 30,
    system_audio: true,
    microphone: false,
    open_clip: true,
});

/// 菜单项 id（扩展前缀避免与其它扩展贡献碰撞）。
const MENU_STOP_ID: &str = "screenshot_record_stop";

// ---------------------------------------------------------------------------
// 生命周期
// ---------------------------------------------------------------------------

/// 扩展 setup 内调用：注册 finish 回调 / 菜单栏贡献 / 录屏快捷键钩子。
#[cfg(target_os = "macos")]
pub fn setup(app: &AppHandle) {
    let _ = APP.set(app.clone());
    // SAFETY: 回调为 extern "C" 函数指针，生命周期与进程一致；仅设置静态函数指针
    unsafe {
        voidnix_record_set_finish_callback(on_record_finish);
    }
    crate::runtime::menubar::register(MenuBarContribution {
        title: "录屏",
        order: 110,
        build: Arc::new(build_menu),
        on_event: Arc::new(on_menu_event),
    });
}

#[cfg(not(target_os = "macos"))]
pub fn setup(_app: &AppHandle) {}

/// 录制中（Starting / Cancelling / Stopping 均算，防状态窗口期二次进入）。
pub fn is_busy() -> bool {
    PHASE.load(Ordering::SeqCst) != PHASE_IDLE
}

/// 正在收尾（native 落盘/看门狗窗口）。
pub fn is_stopping() -> bool {
    PHASE.load(Ordering::SeqCst) == PHASE_STOPPING
}

/// 取消在途启动（overlay 关闭 / 截屏快捷键在 STARTING 时）。返回 true = 已把
/// STARTING 抢成 CANCELLING；启动命令收尾时
/// 检测到该相位即 abort + 删文件。非 STARTING 相位返回 false（调用方忽略）。
pub fn cancel_pending_start() -> bool {
    PHASE
        .compare_exchange(
            PHASE_STARTING,
            PHASE_CANCELLING,
            Ordering::SeqCst,
            Ordering::SeqCst,
        )
        .is_ok()
}

// ---------------------------------------------------------------------------
// 菜单栏
// ---------------------------------------------------------------------------

fn build_menu(_app: &AppHandle) -> Vec<MenuEntry> {
    let phase = PHASE.load(Ordering::SeqCst);
    if phase != PHASE_RECORDING && phase != PHASE_STOPPING {
        return Vec::new();
    }
    let elapsed = {
        let guard = lock_or_recover(&SESSION);
        guard
            .as_ref()
            .map(|session| session.started.elapsed().as_secs())
            .unwrap_or(0)
    };
    let label = if phase == PHASE_STOPPING {
        "正在保存录屏…".to_string()
    } else {
        format!("停止录屏  {}", format_elapsed(elapsed))
    };
    vec![MenuEntry::Item {
        id: MENU_STOP_ID.to_string(),
        label,
        enabled: phase == PHASE_RECORDING,
    }]
}

fn on_menu_event(app: &AppHandle, id: &str) {
    if id == MENU_STOP_ID {
        stop(app);
    }
}

/// 录制期间每秒刷新菜单栏计时（刷新链自身幂等，录制结束即退出）。
///
/// 只负责「在录时的文案更新」：开始/结束的菜单重建分别由 start command 与
/// finish 完成，ticker 退出时不再补刷新；图标隐藏时菜单不渲染，跳过重建。
fn start_menu_ticker(app: &AppHandle) {
    let generation = TICKER_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    std::thread::spawn(move || {
        while PHASE.load(Ordering::SeqCst) == PHASE_RECORDING
            && TICKER_GEN.load(Ordering::SeqCst) == generation
        {
            if crate::runtime::menubar::is_icon_visible() {
                crate::runtime::menubar::refresh(&app);
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    });
}

fn format_elapsed(secs: u64) -> String {
    if secs >= 3600 {
        format!("{}:{:02}:{:02}", secs / 3600, (secs % 3600) / 60, secs % 60)
    } else {
        format!("{:02}:{:02}", secs / 60, secs % 60)
    }
}

// ---------------------------------------------------------------------------
// 命令
// ---------------------------------------------------------------------------

/// 录屏选项同步（main 窗口 config watch 驱动；savePath 与截屏共用，空 = ~/Downloads）。
#[tauri::command]
pub fn set_screenshot_record_config(
    save_path: String,
    fps: u32,
    system_audio: bool,
    microphone: bool,
    open_clip: bool,
) {
    let mut options = lock_or_recover(&OPTIONS);
    options.save_path = save_path;
    options.fps = fps.clamp(15, 60);
    options.system_audio = system_audio;
    options.microphone = microphone;
    options.open_clip = open_clip;
}

/// 截屏/录屏共用保存位置（空 = ~/Downloads，与截屏默认一致）。
pub fn save_path() -> String {
    lock_or_recover(&OPTIONS).save_path.clone()
}

/// overlay 录屏弹出菜单的初始选项（当前 Rust 权威值）。
pub fn current_prefs() -> super::ffi::RecordPrefs {
    let options = lock_or_recover(&OPTIONS);
    super::ffi::RecordPrefs {
        system_audio: options.system_audio,
        microphone: options.microphone,
        fps: options.fps,
        open_clip: options.open_clip,
    }
}

/// 开始录制当前截屏会话选区（overlay 工具栏「录屏」按钮触发）。
#[tauri::command]
pub async fn start_screen_recording(
    app: AppHandle,
    sel_x: f64,
    sel_y: f64,
    sel_w: f64,
    sel_h: f64,
    scale: f64,
    prefs: super::ffi::RecordPrefs,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        // 弹出菜单选项即本次录制参数，同时回写 Rust 权威值（下次 overlay 初始值）
        {
            let mut options = lock_or_recover(&OPTIONS);
            options.fps = prefs.fps.clamp(15, 60);
            options.system_audio = prefs.system_audio;
            options.microphone = prefs.microphone;
            options.open_clip = prefs.open_clip;
        }
        if PHASE
            .compare_exchange(
                PHASE_IDLE,
                PHASE_STARTING,
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .is_err()
        {
            return Err("录屏已在进行中".to_string());
        }
        match start_inner(sel_x, sel_y, sel_w, sel_h, scale).await {
            Ok(path) => {
                *lock_or_recover(&SESSION) = Some(RecordSession {
                    path,
                    started: Instant::now(),
                });
                // STARTING → RECORDING 必须是 CAS：启动期间可能已被取消（CANCELLING）
                // 或流已失败（finish 抢先回到 IDLE）。两者都不能进入 Recording——
                // 否则会留下「界面认为在录、原生侧已停止」的悬挂态。
                match PHASE.compare_exchange(
                    PHASE_STARTING,
                    PHASE_RECORDING,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                ) {
                    Ok(_) => {
                        crate::runtime::menubar::refresh(&app);
                        start_menu_ticker(&app);
                        Ok(())
                    }
                    Err(current) => {
                        // current == IDLE：finish 已抢先收束（session 可能已被它取走），
                        // 不能再写 PHASE（避免踩到随后新启动的另一轮）
                        let cancelled = current == PHASE_CANCELLING;
                        if cancelled {
                            // 原生侧已经在录：强拆（停流 + 清单例），不发 finish 事件
                            // SAFETY: abort 幂等，只释放原生资源
                            unsafe {
                                voidnix_record_abort();
                            }
                            PHASE.store(PHASE_IDLE, Ordering::SeqCst);
                        }
                        // finish 抢先时会自行取走并处理 session；仍在说明是取消路径
                        // 或写入前就已收尾，需要删除本次创建的半成品。
                        if let Some(session) = lock_or_recover(&SESSION).take() {
                            let _ = std::fs::remove_file(&session.path);
                        }
                        Err(if cancelled {
                            "已取消录屏".to_string()
                        } else {
                            "录屏启动失败".to_string()
                        })
                    }
                }
            }
            Err(e) => {
                // 取消窗口内 native start 也可能返回错误：统一以取消语义收束
                if PHASE.swap(PHASE_IDLE, Ordering::SeqCst) == PHASE_CANCELLING {
                    return Err("已取消录屏".to_string());
                }
                Err(e)
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, sel_x, sel_y, sel_w, sel_h, scale, prefs);
        Err("仅支持 macOS".to_string())
    }
}

#[cfg(target_os = "macos")]
async fn start_inner(
    sel_x: f64,
    sel_y: f64,
    sel_w: f64,
    sel_h: f64,
    scale: f64,
) -> Result<PathBuf, String> {
    if !crate::platform::permission::check_screen_recording() {
        return Err("录屏失败：请在「系统设置 → 隐私与安全性 → 录屏」中授权".to_string());
    }
    if sel_w < 8.0 || sel_h < 8.0 {
        return Err("录制区域过小".to_string());
    }
    let surface = super::session::capture_surface().ok_or_else(|| "截屏会话已结束".to_string())?;
    let (dir_raw, fps, system_audio, microphone) = {
        let options = lock_or_recover(&OPTIONS);
        (
            options.save_path.clone(),
            options.fps,
            options.system_audio,
            options.microphone,
        )
    };
    let dir = resolve_record_dir(&dir_raw);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建录屏目录失败：{e}"))?;
    let path = unique_record_path(&dir);
    let c_path =
        std::ffi::CString::new(path.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
    let path_for_thread = path.clone();

    // SCK 的 ShareableContent / startCapture 是异步 completion，桥内用信号量同步；
    // 必须离开 tokio worker（阻塞可达 20s），spawn_blocking 承接。
    tauri::async_runtime::spawn_blocking(move || {
        let mut error_buffer = [0i8; 512];
        // SAFETY: c_path 为非空 CString；错误缓冲区为栈上定长数组，桥内 strlcpy 截断
        let ok = unsafe {
            voidnix_record_start(
                surface.display_id,
                sel_x,
                sel_y,
                sel_w,
                sel_h,
                scale,
                fps as i32,
                system_audio,
                microphone,
                c_path.as_ptr(),
                error_buffer.as_mut_ptr(),
                error_buffer.len(),
            )
        };
        if ok {
            Ok(())
        } else {
            // SAFETY: 失败路径桥保证 error_buffer 为 NUL 结尾字符串
            let message = unsafe { std::ffi::CStr::from_ptr(error_buffer.as_ptr()) }
                .to_string_lossy()
                .into_owned();
            Err(if message.is_empty() {
                "开始录屏失败".to_string()
            } else {
                message
            })
        }
    })
    .await
    .map_err(|e| e.to_string())??;

    Ok(path_for_thread)
}

/// 停止录制（截屏快捷键 / 菜单栏）。收尾结果经 finish 回调异步回报。
pub fn stop(app: &AppHandle) {
    // 启动尚未完成：取消（native start 返回后由命令 abort 并删文件）
    if cancel_pending_start() {
        return;
    }
    if PHASE
        .compare_exchange(
            PHASE_RECORDING,
            PHASE_STOPPING,
            Ordering::SeqCst,
            Ordering::SeqCst,
        )
        .is_err()
    {
        return;
    }
    #[cfg(target_os = "macos")]
    // SAFETY: 桥内录屏实例单例，stop 幂等（实例不存在直接返回）
    unsafe {
        voidnix_record_stop();
    }
    crate::runtime::menubar::refresh(app);

    // 兜底看门狗：桥内回调若因异常未到达，状态也必须收束（否则永远无法再录）。
    // 200ms 轮询而非直接睡 20s——正常收尾是毫秒级，线程立即退出不残留长睡眠。
    std::thread::spawn(move || {
        for _ in 0..100 {
            std::thread::sleep(Duration::from_millis(200));
            if PHASE.load(Ordering::SeqCst) != PHASE_STOPPING {
                return;
            }
        }
        // 桥内看门狗（15s）应先到；这里兜底的是「回调本身没回来」的极端情况：
        // 先强拆原生实例（停流 + 清 g_recorder，防下次启动被误判为在录），再收束状态
        #[cfg(target_os = "macos")]
        // SAFETY: abort 幂等（实例不存在直接返回），只释放原生资源，不回调
        unsafe {
            voidnix_record_abort();
        }
        finish(false, 0, "结束录屏超时".to_string());
    });
}

#[cfg(target_os = "macos")]
extern "C" fn on_record_finish(ok: i32, duration_ms: i64, error: *const std::os::raw::c_char) {
    let message = if error.is_null() {
        String::new()
    } else {
        // SAFETY: 桥内以 NUL 结尾 NSString.UTF8String 传入
        unsafe { std::ffi::CStr::from_ptr(error) }
            .to_string_lossy()
            .into_owned()
    };
    finish(ok == 1, duration_ms, message);
}

/// 唯一终态收束：清状态 / 刷新菜单 / 通知前端（成功交剪辑，失败 toast）。
fn finish(ok: bool, duration_ms: i64, error: String) {
    if PHASE.swap(PHASE_IDLE, Ordering::SeqCst) == PHASE_IDLE {
        // 已被其它终态路径处理（幂等）
        return;
    }
    let session = { lock_or_recover(&SESSION).take() };
    let Some(app) = APP.get().cloned() else {
        return;
    };
    crate::runtime::menubar::refresh(&app);
    // 菜单点击触发的 refresh 在菜单仍打开时会被框架跳过（改为 500ms 重试）；收尾
    // 回调常与菜单关闭同帧到达，补一拍延迟重建，确保「停止录屏」项在下次打开前消失。
    {
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(700));
            if PHASE.load(Ordering::SeqCst) == PHASE_IDLE {
                crate::runtime::menubar::refresh(&app);
            }
        });
    }
    let Some(session) = session else {
        return;
    };
    let duration = if duration_ms > 0 {
        duration_ms as u64
    } else {
        session.started.elapsed().as_millis() as u64
    };
    // 收尾失败（moov 超时 / 停流报错）不删用户数据：文件非空就按 saved 上报并
    // 附 warning，交给剪辑台尝试打开（可能仍可播）。只有空文件才按失败清理。
    let has_content = std::fs::metadata(&session.path)
        .map(|metadata| metadata.len() > 0)
        .unwrap_or(false);
    if ok || has_content {
        let mut payload = serde_json::json!({
            "path": session.path.to_string_lossy(),
            "durationMs": duration,
        });
        if !ok {
            let warning = if error.is_empty() {
                "录屏收尾异常".to_string()
            } else {
                error
            };
            payload["warning"] = serde_json::Value::String(warning);
        }
        let _ = app.emit("screenshot-recording-saved", payload);
    } else {
        let _ = std::fs::remove_file(&session.path);
        let message = if error.is_empty() {
            "录屏失败".to_string()
        } else {
            error
        };
        let _ = app.emit("screenshot-recording-failed", message);
    }
}

// ---------------------------------------------------------------------------
// 路径
// ---------------------------------------------------------------------------

fn resolve_record_dir(raw: &str) -> PathBuf {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        // 与截屏默认一致：一个保存位置服务两条出口
        return dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Downloads");
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        return dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(rest);
    }
    PathBuf::from(trimmed)
}

/// `recording_<unix-ms>.mp4`；同毫秒冲突时追加序号（不覆盖已有文件）。
fn unique_record_path(dir: &Path) -> PathBuf {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let mut path = dir.join(format!("recording_{ts}.mp4"));
    let mut suffix = 1u32;
    while path.exists() {
        path = dir.join(format!("recording_{ts}-{suffix}.mp4"));
        suffix += 1;
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_pending_start_is_scoped_to_starting() {
        PHASE.store(PHASE_IDLE, Ordering::SeqCst);
        assert!(!cancel_pending_start());
        PHASE.store(PHASE_RECORDING, Ordering::SeqCst);
        assert!(!cancel_pending_start());
        PHASE.store(PHASE_STARTING, Ordering::SeqCst);
        assert!(cancel_pending_start());
        assert_eq!(PHASE.load(Ordering::SeqCst), PHASE_CANCELLING);
        PHASE.store(PHASE_IDLE, Ordering::SeqCst);
    }

    #[test]
    fn elapsed_formatting() {
        assert_eq!(format_elapsed(0), "00:00");
        assert_eq!(format_elapsed(65), "01:05");
        assert_eq!(format_elapsed(3661), "1:01:01");
    }

    #[test]
    fn empty_save_path_falls_back_to_downloads() {
        let dir = resolve_record_dir("");
        assert!(dir.ends_with("Downloads"));
        let dir = resolve_record_dir("~/Videos");
        assert!(dir.ends_with("Videos"));
        let dir = resolve_record_dir("/tmp/voidnix-record");
        assert_eq!(dir, PathBuf::from("/tmp/voidnix-record"));
    }

    #[test]
    fn unique_path_has_mp4_extension() {
        let dir = std::env::temp_dir().join(format!("voidnix-record-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = unique_record_path(&dir);
        assert_eq!(path.extension().and_then(|e| e.to_str()), Some("mp4"));
        std::fs::write(&path, b"x").unwrap();
        let second = unique_record_path(&dir);
        assert_ne!(path, second);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
