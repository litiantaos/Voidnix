//! 剪辑会话：独立剪辑窗口生命周期、待处理输入、媒体元数据探测、时间轴素材生成。
//!
//! 剪辑窗口是独立 `WebviewWindow`（clip.html 入口，overlay 标题栏、可缩放），
//! 与启动器主窗解耦——剪辑是长时工作台，不受主窗 720 宽与 auto 高度机制约束。
//! 预览经本地 HTTP Range 服务（见 media_server.rs），导出复用 job.rs 的 ffmpeg 外壳。

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use super::core::{self, FfmpegBins};

const WINDOW_LABEL: &str = "clip";
const WINDOW_WIDTH: f64 = 1180.0;
const WINDOW_HEIGHT: f64 = 780.0;
const WINDOW_MIN_WIDTH: f64 = 900.0;
const WINDOW_MIN_HEIGHT: f64 = 600.0;

/// 窗口打开时登记的待处理输入：窗口前端挂载后经 `video_clip_take_inputs` 取走。
static PENDING: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

fn pending() -> &'static Mutex<Vec<String>> {
    PENDING.get_or_init(|| Mutex::new(Vec::new()))
}

fn temp_clip_dir() -> PathBuf {
    std::env::temp_dir().join("voidnix").join("clip")
}

/// 清理剪辑窗口产生的临时素材（时间轴精灵图 + 导出工作目录）。窗口销毁时调用，
/// 与启动兜底（storage::cleanup_all_voidnix_temps）同源。
fn cleanup_temps() {
    crate::runtime::storage::cleanup_clip_temps(&std::env::temp_dir().join("voidnix"));
}

// ---------------------------------------------------------------------------
// 元数据探测
// ---------------------------------------------------------------------------

/// 镜头元数据（前端剪辑会话的单一真相源）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipMeta {
    pub path: String,
    pub duration_us: i64,
    pub width: u32,
    pub height: u32,
    /// 应用 rotation 后的展示尺寸（竖拍交换宽高）
    pub display_width: u32,
    pub display_height: u32,
    pub fps_num: u32,
    pub fps_den: u32,
    pub video_codec: String,
    pub codec_tag: String,
    pub pix_fmt: String,
    pub rotation: i32,
    pub has_audio: bool,
    pub audio_codec: String,
    pub sample_rate: u32,
    pub channels: u32,
    pub size_bytes: u64,
    pub container: String,
    /// 能否直接喂给 `<video>`；false 需先重封装修 / 生成预览代理
    pub previewable: bool,
}

fn parse_fps(raw: &str) -> (u32, u32) {
    let mut parts = raw.split('/');
    let num = parts.next().and_then(|s| s.trim().parse::<u32>().ok());
    let den = parts.next().and_then(|s| s.trim().parse::<u32>().ok());
    match (num, den) {
        (Some(n), Some(d)) if n > 0 && d > 0 => (n, d),
        _ => (30, 1),
    }
}

fn ffprobe_json(bins: &FfmpegBins, path: &Path) -> Result<serde_json::Value, String> {
    let out = Command::new(&bins.ffprobe)
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(path)
        .output()
        .map_err(|e| format!("ffprobe 调用失败：{e}"))?;
    if !out.status.success() {
        return Err(format!(
            "ffprobe 解析失败：{}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("ffprobe 输出解析失败：{e}"))
}

pub(crate) fn rotation_of(stream: &serde_json::Value) -> i32 {
    if let Some(list) = stream.get("side_data_list").and_then(|v| v.as_array()) {
        for sd in list {
            if let Some(r) = sd.get("rotation").and_then(|v| v.as_f64()) {
                return r.round() as i32;
            }
        }
    }
    stream
        .pointer("/tags/rotate")
        .and_then(|v| v.as_str())
        .and_then(|s| s.trim().parse::<i32>().ok())
        .unwrap_or(0)
}

fn is_previewable(path: &Path, container: &str, codec: &str, tag: &str) -> bool {
    // webm 与 mkv 的 ffprobe format_name 同为 `matroska,webm`，只能靠扩展名 + 编码区分：
    // WebKit 只认 webm 容器里的 VP8/VP9（VP9 webm 实测可播；MKV 一律不可播）
    if container.contains("matroska") {
        let is_webm = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("webm"))
            .unwrap_or(false);
        return is_webm && matches!(codec, "vp8" | "vp9");
    }
    match codec {
        "h264" => true,
        // ffmpeg 默认写 hev1 tag，WebKit 只认 hvc1（需 -c copy -tag:v hvc1 重封装）
        "hevc" => tag == "hvc1",
        "prores" => true,
        _ => false,
    }
}

pub fn probe_one(app: &AppHandle, path: &str) -> Result<ClipMeta, String> {
    let bins = core::resolve_bins(app).ok_or("FFmpeg 未就绪，请先下载核心")?;
    let p = Path::new(path);
    if !crate::platform::path_guard::validate(p) {
        return Err(format!("路径不安全或不存在：{path}"));
    }
    let size_bytes = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    let json = ffprobe_json(&bins, p)?;

    let duration_us = json
        .pointer("/format/duration")
        .and_then(|v| {
            v.as_str()
                .and_then(|s| s.parse::<f64>().ok())
                .or_else(|| v.as_f64())
        })
        .map(|secs| (secs * 1_000_000.0).round() as i64)
        .unwrap_or(0);

    let container = json
        .pointer("/format/format_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let streams = json
        .get("streams")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut video: Option<&serde_json::Value> = None;
    let mut audio: Option<&serde_json::Value> = None;
    for s in &streams {
        match s.get("codec_type").and_then(|v| v.as_str()) {
            Some("video") if video.is_none() => video = Some(s),
            Some("audio") if audio.is_none() => audio = Some(s),
            _ => {}
        }
    }
    let video = video.ok_or("未找到视频流")?;

    let width = video.get("width").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let height = video.get("height").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let rotation = rotation_of(video);
    let swapped = rotation.abs() % 180 == 90;
    let (display_width, display_height) = if swapped {
        (height, width)
    } else {
        (width, height)
    };

    let (fps_num, fps_den) = video
        .get("avg_frame_rate")
        .and_then(|v| v.as_str())
        .filter(|s| *s != "0/0")
        .map(parse_fps)
        .unwrap_or_else(|| {
            video
                .get("r_frame_rate")
                .and_then(|v| v.as_str())
                .map(parse_fps)
                .unwrap_or((30, 1))
        });

    let video_codec = video
        .get("codec_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let codec_tag = video
        .get("codec_tag_string")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let pix_fmt = video
        .get("pix_fmt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let previewable = is_previewable(p, &container, &video_codec, &codec_tag);

    Ok(ClipMeta {
        path: path.to_string(),
        duration_us,
        width,
        height,
        display_width,
        display_height,
        fps_num,
        fps_den,
        video_codec,
        codec_tag,
        pix_fmt,
        rotation,
        has_audio: audio.is_some(),
        audio_codec: audio
            .and_then(|a| a.get("codec_name"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        sample_rate: audio
            .and_then(|a| a.get("sample_rate"))
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0),
        channels: audio
            .and_then(|a| a.get("channels"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        size_bytes,
        container,
        previewable,
    })
}

/// 探测一批视频文件元数据（顺序探测，剪辑一次最多几个文件）。
#[tauri::command]
pub async fn video_clip_probe(app: AppHandle, paths: Vec<String>) -> Result<Vec<ClipMeta>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut out = Vec::with_capacity(paths.len());
        for path in paths {
            out.push(probe_one(&app, &path)?);
        }
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// 时间轴素材（缩略图精灵图）
// ---------------------------------------------------------------------------

/// 一条时间轴缩略图精灵（横向单行布局，前端按帧切图）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipTimeline {
    /// 临时 PNG 绝对路径（前端 convertFileSrc 后交给 <canvas> drawImage）
    pub sprite_path: String,
    pub frame_width: u32,
    pub frame_height: u32,
    pub count: u32,
    /// 相邻缩略图的时间间隔（微秒）
    pub interval_us: i64,
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn sprite_cache_path(path: &Path, count: u32, frame_width: u32, frame_height: u32) -> PathBuf {
    let mtime = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let key = format!(
        "{}|{}|{}|{}|{}",
        path.display(),
        mtime,
        count,
        frame_width,
        frame_height
    );
    temp_clip_dir().join(format!("tl_{:016x}.png", fnv1a(key.as_bytes())))
}

/// 生成时间轴缩略图精灵：单进程一次解码（fps + tile），缓存命中直接返回。
#[tauri::command]
pub async fn video_clip_timeline(
    app: AppHandle,
    path: String,
    duration_us: i64,
    count: u32,
    frame_width: u32,
    frame_height: u32,
) -> Result<ClipTimeline, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let bins = core::resolve_bins(&app).ok_or("FFmpeg 未就绪，请先下载核心")?;
        let src = PathBuf::from(&path);
        if !crate::platform::path_guard::validate(&src) {
            return Err("路径不安全或不存在".into());
        }
        if duration_us <= 0 || count == 0 || frame_width == 0 || frame_height == 0 {
            return Err("时间轴参数无效".into());
        }
        let count = count.clamp(1, 200);
        let frame_width = frame_width.clamp(16, 640);
        let frame_height = frame_height.clamp(16, 640);
        let dir = temp_clip_dir();
        std::fs::create_dir_all(&dir).map_err(|e| format!("临时目录创建失败：{e}"))?;
        let out_path = sprite_cache_path(&src, count, frame_width, frame_height);
        let interval_us = duration_us / count as i64;

        if !out_path.is_file() {
            let duration_secs = duration_us as f64 / 1_000_000.0;
            // fps=N/duration：严格按总量取 count 帧后拼成单行精灵图
            let fps_expr = format!("{count}/{duration_secs:.6}");
            let filter =
                format!("fps={fps_expr},scale={frame_width}:{frame_height},tile={count}x1");
            let output = Command::new(&bins.ffmpeg)
                .args(["-v", "error", "-y", "-i"])
                .arg(&src)
                .args(["-vf", &filter, "-frames:v", "1"])
                .arg(&out_path)
                .output()
                .map_err(|e| format!("ffmpeg 调用失败：{e}"))?;
            if !output.status.success() || !out_path.is_file() {
                let err = String::from_utf8_lossy(&output.stderr);
                return Err(format!("时间轴缩略图生成失败：{}", err.trim()));
            }
        }

        Ok(ClipTimeline {
            sprite_path: out_path.to_string_lossy().to_string(),
            frame_width,
            frame_height,
            count: count.min(200),
            interval_us,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// 静音 / 停顿分析（一键去停顿）
// ---------------------------------------------------------------------------

/// 静音区间（微秒，[start, end)）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SilenceRange {
    pub start_us: i64,
    pub end_us: i64,
}

/// 从 silencedetect 的 stderr 输出解析静音区间（未闭合的尾部静音补到素材结尾）。
fn parse_silence(stderr: &str, duration_us: i64) -> Vec<SilenceRange> {
    let mut ranges = Vec::new();
    let mut open_start: Option<i64> = None;
    for line in stderr.lines() {
        if let Some(rest) = line.split("silence_start:").nth(1) {
            if let Ok(secs) = rest.trim().parse::<f64>() {
                open_start = Some((secs * 1_000_000.0).round() as i64);
            }
        } else if let Some(rest) = line.split("silence_end:").nth(1) {
            let secs = rest
                .split_whitespace()
                .next()
                .and_then(|v| v.parse::<f64>().ok());
            if let (Some(start), Some(secs)) = (open_start.take(), secs) {
                ranges.push(SilenceRange {
                    start_us: start,
                    end_us: (secs * 1_000_000.0).round() as i64,
                });
            }
        }
    }
    if let Some(start) = open_start {
        if duration_us > start {
            ranges.push(SilenceRange {
                start_us: start,
                end_us: duration_us,
            });
        }
    }
    ranges
}

/// 分析静音/停顿区间（「一键去停顿」的数据源）。
///
/// silencedetect 默认参数是 -60dB / 2s，对口播素材过于宽松；默认 -35dB / 300ms。
/// 前端按「保留气口」把静音区间收窄为删减集合，再由用户审阅应用（非破坏）。
#[tauri::command]
pub async fn video_clip_analyze(
    app: AppHandle,
    path: String,
    noise_db: Option<f64>,
    min_silence_ms: Option<u32>,
) -> Result<Vec<SilenceRange>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let bins = core::resolve_bins(&app).ok_or("FFmpeg 未就绪，请先下载核心")?;
        let src = PathBuf::from(&path);
        if !crate::platform::path_guard::validate(&src) {
            return Err("路径不安全或不存在".into());
        }
        let duration_us = probe_one(&app, &path)?.duration_us;
        let noise = noise_db.unwrap_or(-35.0);
        let min_ms = min_silence_ms.unwrap_or(300).clamp(50, 30_000);
        let filter = format!(
            "silencedetect=noise={noise}dB:d={:.3}",
            min_ms as f64 / 1000.0
        );
        let out = Command::new(&bins.ffmpeg)
            .args(["-hide_banner", "-nostdin", "-i"])
            .arg(&src)
            // -vn 跳过视频解码（silencedetect 只消费音轨），长素材分析快约一个数量级
            .args(["-vn", "-af", &filter, "-f", "null", "-"])
            .output()
            .map_err(|e| format!("静音分析失败：{e}"))?;
        let text = String::from_utf8_lossy(&out.stderr);
        Ok(parse_silence(&text, duration_us))
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// 窗口生命周期
// ---------------------------------------------------------------------------

fn build_window(app: &AppHandle) -> Result<(), String> {
    // 剪辑窗口期间切 **Regular** 激活策略：Accessory（LSUIElement）应用的窗口在台前调度
    // 中是常驻浮层（不进 stage、切走不收起、悬浮在其它应用上方）；Regular 让剪辑窗
    // 拥有独立 stage / Dock 图标 / ⌘Tab 归属， behave like 普通文档应用。关窗
    // （Destroyed）时还原 Accessory，回到启动器形态。运行时切换为 Apple 支持的用法。
    if let Err(e) = app.set_activation_policy(tauri::ActivationPolicy::Regular) {
        log::warn!("[clip] set Regular activation policy failed: {e}");
    }
    // dev 裸二进制无 bundle 图标，Dock 会显示占位——用编译期内置的应用图标补设
    //（build_window 恒在主线程运行）
    if let Some(icon) = app.default_window_icon() {
        crate::platform::window::set_dock_icon(icon.rgba(), icon.width(), icon.height());
    }
    let url = WebviewUrl::App("clip.html".into());
    // overlay 标题栏：系统红绿灯悬浮在内容左上、标题文本隐藏、无标题栏色条，内容顶到
    // 窗口顶部；拖动由前端头部空白区 mousedown → startDragging 承担（macOS 双击
    // internal_toggle_maximize 的 data-tauri-drag-region 陷阱不适用：本窗可缩放，双击
    // 缩放本就是期望行为，但手动拖动同样简单且与主窗范式一致）
    let builder = WebviewWindowBuilder::new(app, WINDOW_LABEL, url)
        .title("剪辑")
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true)
        .inner_size(WINDOW_WIDTH, WINDOW_HEIGHT)
        .min_inner_size(WINDOW_MIN_WIDTH, WINDOW_MIN_HEIGHT)
        .resizable(true)
        .center()
        .focused(true)
        .visible(true)
        .accept_first_mouse(true);
    let window = builder
        .build()
        .map_err(|e| format!("创建剪辑窗口失败：{e}"))?;
    // 跟随应用主题：explicit 设置 appearance 交给全局 set_window_appearance 驱动，
    // 这里按缓存补一次（窗口创建可能晚于主窗主题切换）。
    crate::platform::window::apply_cached_appearance(&window);

    let app_for_event = app.clone();
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Destroyed) {
            // 前端 onBeforeUnmount 随 webview 销毁不可靠执行，取消导出以 Rust 侧为准
            super::clip_export::cancel_export();
            super::media_server::stop();
            cleanup_temps();
            if let Ok(mut p) = pending().lock() {
                p.clear();
            }
            // 还原启动器形态：Dock 图标消失、退出台前调度独立 stage、恢复纯菜单栏应用
            if let Err(e) = app_for_event.set_activation_policy(tauri::ActivationPolicy::Accessory)
            {
                log::warn!("[clip] restore Accessory activation policy failed: {e}");
            }
        }
    });
    Ok(())
}

/// 打开（或聚焦）剪辑窗口并登记待处理输入。
///
/// 已有窗口时：登记输入并广播 `clip-inputs-changed`，由前端重新加载。
#[tauri::command]
pub async fn video_clip_open(app: AppHandle, paths: Vec<String>) -> Result<(), String> {
    let filtered: Vec<String> = paths.into_iter().filter(|p| !p.trim().is_empty()).collect();
    if filtered.is_empty() {
        return Err("未选择视频文件".into());
    }
    {
        let mut p = pending().lock().map_err(|_| "剪辑输入锁失效")?;
        *p = filtered.clone();
    }

    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        let _ = app.emit_to(WINDOW_LABEL, "clip-inputs-changed", &filtered);
        let _ = window.show();
        focus_and_refocus_later(&app);
        return Ok(());
    }

    let app_for_main = app.clone();
    let (tx, rx) = tokio::sync::oneshot::channel::<Result<(), String>>();
    app.run_on_main_thread(move || {
        let result = build_window(&app_for_main);
        if result.is_ok() {
            focus_and_refocus_later(&app_for_main);
        }
        let _ = tx.send(result);
    })
    .map_err(|e| format!("主线程调度失败：{e}"))?;
    // 全程 async 等待（oneshot + timeout），不阻塞 tokio worker 线程
    tokio::time::timeout(std::time::Duration::from_secs(15), rx)
        .await
        .map_err(|_| "创建剪辑窗口超时".to_string())?
        .map_err(|_| "剪辑窗口创建结果通道关闭".to_string())??;
    Ok(())
}

/// 激活 app 并让剪辑窗口成为 key window（键盘才能进入 webview）。
///
/// **仅在主线程调用**：`activate_app` 依赖 MainThreadMarker，`make_key_window` 裸调
/// NSWindow——在 tokio 线程直接调前者是静默 no-op（表现为窗口打开后按键全无反应）、
/// 后者违反 AppKit 线程纪律。app 是 Accessory（LSUIElement 无 Dock 图标），从后台
/// 打开必须显式激活（macOS 14+ 可能忽略旧 `activateIgnoringOtherApps`，activate_app
/// 内已补 `NSRunningApplication.activateWithOptions` 兜底）。
fn focus_clip_window(app: &AppHandle) {
    crate::platform::focus::activate_app();
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        crate::platform::window::make_key_window(&window);
        let _ = window.set_focus();
    }
}

/// 聚焦剪辑窗口，并补一拍延迟二次聚焦（300ms 后）。
///
/// openClipEditor 的时序是 hideWindow（`restore_captured` 会激活**前一个前台 app**）
/// → video_clip_open 聚焦自己——两次系统激活事务竞态，本窗口可能输掉 key 归属
///（表现：窗口可见可点但键盘全无响应，合成键入落到原前台 app）。主线程立即聚焦
/// 一次 + 短延迟后再聚焦一次，确定性赢下竞态；后续窗口点击也会自然夺回 key。
fn focus_and_refocus_later(app: &AppHandle) {
    focus_clip_window(app);
    let app_later = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(300));
        let value = app_later.clone();
        let _ = app_later.run_on_main_thread(move || focus_clip_window(&value));
    });
}

/// 取走窗口打开时登记的待处理输入（前端挂载后调用一次）。
#[tauri::command]
pub fn video_clip_take_inputs() -> Vec<String> {
    pending()
        .lock()
        .map(|mut p| std::mem::take(&mut *p))
        .unwrap_or_default()
}

/// 关闭剪辑窗口（前端 Esc / 导出完成后调用）。
#[tauri::command]
pub async fn video_clip_close(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        let _ = window.close();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fps_parsing() {
        assert_eq!(parse_fps("30000/1001"), (30000, 1001));
        assert_eq!(parse_fps("30/1"), (30, 1));
        assert_eq!(parse_fps("0/0"), (30, 1));
        assert_eq!(parse_fps("garbage"), (30, 1));
    }

    #[test]
    fn previewable_matrix() {
        let mp4 = Path::new("/tmp/a.mp4");
        let webm = Path::new("/tmp/a.webm");
        let mkv = Path::new("/tmp/a.mkv");
        assert!(is_previewable(
            mp4,
            "mov,mp4,m4a,3gp,3g2,mj2",
            "h264",
            "avc1"
        ));
        assert!(is_previewable(
            mp4,
            "mov,mp4,m4a,3gp,3g2,mj2",
            "hevc",
            "hvc1"
        ));
        assert!(!is_previewable(
            mp4,
            "mov,mp4,m4a,3gp,3g2,mj2",
            "hevc",
            "hev1"
        ));
        assert!(is_previewable(
            mp4,
            "mov,mp4,m4a,3gp,3g2,mj2",
            "prores",
            "apcn"
        ));
        assert!(!is_previewable(
            mp4,
            "mov,mp4,m4a,3gp,3g2,mj2",
            "av1",
            "av01"
        ));
        // matroska 家族：仅 .webm 的 VP8/VP9 可播
        assert!(!is_previewable(mkv, "matroska,webm", "h264", "avc1"));
        assert!(!is_previewable(mkv, "matroska,webm", "vp9", "vp09"));
        assert!(is_previewable(webm, "matroska,webm", "vp9", "vp09"));
        assert!(!is_previewable(webm, "matroska,webm", "av1", "av01"));
    }

    #[test]
    fn rotation_swaps_display_size() {
        let mut stream = serde_json::json!({"width": 1920, "height": 1080});
        stream["side_data_list"] = serde_json::json!([{ "rotation": -90 }]);
        let r = rotation_of(&stream);
        assert_eq!(r, -90);
        assert_eq!(r.abs() % 180, 90);
    }

    #[test]
    fn silence_parsing() {
        let stderr = "[silencedetect @ 0x1] silence_start: 1.5\n\
[silencedetect @ 0x1] silence_end: 3.25 | silence_duration: 1.75\n\
[silencedetect @ 0x1] silence_start: 8.0\n";
        let ranges = parse_silence(stderr, 10_000_000);
        assert_eq!(ranges.len(), 2);
        assert_eq!(ranges[0].start_us, 1_500_000);
        assert_eq!(ranges[0].end_us, 3_250_000);
        // 未闭合的尾部静音补到素材结尾
        assert_eq!(ranges[1].start_us, 8_000_000);
        assert_eq!(ranges[1].end_us, 10_000_000);
    }

    #[test]
    fn silence_parsing_empty() {
        assert!(parse_silence("", 1_000_000).is_empty());
        assert!(parse_silence("[silencedetect @ 0x1] silence_end: 1\n", 1_000_000).is_empty());
    }

    #[test]
    fn temp_and_cache_paths_are_stable() {
        let p = Path::new("/tmp/x.mp4");
        let a = sprite_cache_path(p, 40, 160, 90);
        let b = sprite_cache_path(p, 40, 160, 90);
        assert_eq!(a, b);
        assert_ne!(a, sprite_cache_path(p, 40, 160, 91));
        assert!(a.to_string_lossy().contains("voidnix/clip"));
    }
}
