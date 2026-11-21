//! 剪辑导出：逐段重编码 → concat 合并（精确档），或单段流拷贝（快速档）。
//!
//! 精确档逐段 encode + concat demuxer `-c copy`：每段独立进程，进度可精确换算；
//! 段规格（分辨率/帧率/音频）在本层统一，concat copy 才能安全。
//! 快速档仅允许单段（多段带裁剪的 copy 段直接 concat 会显形隐藏 GOP 前缀，实测不可用）。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::AppHandle;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

use super::core;

static EXPORT_BUSY: AtomicBool = AtomicBool::new(false);
static EXPORT_CANCEL: OnceLock<Mutex<Option<CancellationToken>>> = OnceLock::new();

fn cancel_slot() -> &'static Mutex<Option<CancellationToken>> {
    EXPORT_CANCEL.get_or_init(|| Mutex::new(None))
}

fn temp_export_dir() -> PathBuf {
    std::env::temp_dir().join("voidnix").join("clip")
}

/// 一个待导出片段（微秒时间轴）。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipSegmentRequest {
    pub path: String,
    pub in_us: i64,
    pub out_us: i64,
    /// 倍速（0.25–8，钳制）：成片时长 = 源时长 / speed；1 = 原速
    #[serde(default = "default_speed")]
    pub speed: f64,
}

fn default_speed() -> f64 {
    1.0
}

/// 导出侧倍速钳制（与前端 SPEED_PRESETS 覆盖域一致，留余量到 8）。
fn clamp_speed(speed: f64) -> f64 {
    if !speed.is_finite() || speed <= 0.0 {
        return 1.0;
    }
    speed.clamp(0.25, 8.0)
}

/// atempo 链：单实例只接受 [0.5, 2]，超出用链式分解（4 → 2×2，0.25 → 0.5×2）。
fn atempo_chain(speed: f64) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut remaining = speed;
    while remaining > 2.0 + f64::EPSILON {
        parts.push("atempo=2.0".into());
        remaining /= 2.0;
    }
    while remaining < 0.5 - f64::EPSILON {
        parts.push("atempo=0.5".into());
        remaining /= 0.5;
    }
    parts.push(format!("atempo={remaining:.4}"));
    parts.join(",")
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipExportRequest {
    pub segments: Vec<ClipSegmentRequest>,
    /// 输出文件绝对路径；空/None = 源文件旁 `{stem}-cut.mp4`
    #[serde(default)]
    pub output_path: Option<String>,
    /// "precise"（重编码，默认）| "copy"（仅单段，无损快速）
    #[serde(default = "default_mode")]
    pub mode: String,
    /// 统一高度：0/source = 保持源尺寸，1080 / 720 等比缩放
    #[serde(default)]
    pub target_height: u32,
}

fn default_mode() -> String {
    "precise".to_string()
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ClipExportEvent {
    Started {
        total_us: i64,
        segments: usize,
    },
    Progress {
        /// 0–100
        percent: f64,
        stage: String,
    },
    Done {
        output_path: String,
        size_bytes: u64,
        duration_us: i64,
    },
    Error {
        message: String,
    },
}

fn emit(on_event: &Channel<ClipExportEvent>, event: ClipExportEvent) {
    let _ = on_event.send(event);
}

/// 生成与源文件同目录的不冲突输出路径：`{stem}-cut.mp4` / `-cut-1.mp4` …
fn build_output_path(first_segment: &Path, explicit: Option<&str>) -> Result<PathBuf, String> {
    if let Some(raw) = explicit.map(str::trim).filter(|s| !s.is_empty()) {
        let p = PathBuf::from(raw);
        if !p.is_absolute() {
            return Err("输出路径必须是绝对路径".into());
        }
        let parent = p.parent().ok_or("输出路径无效")?;
        if !parent.is_dir() {
            return Err("输出目录不存在".into());
        }
        if !crate::platform::path_guard::validate(parent) {
            return Err("输出目录不安全".into());
        }
        return Ok(p);
    }
    let dir = first_segment
        .parent()
        .ok_or("源文件目录无效")?
        .to_path_buf();
    let stem = first_segment
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("video");
    let base = dir.join(format!("{stem}-cut.mp4"));
    if !base.exists() {
        return Ok(base);
    }
    for index in 1..100 {
        let candidate = dir.join(format!("{stem}-cut-{index}.mp4"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err("输出文件已存在且无法生成唯一名称".into())
}

fn seconds(micros: i64) -> String {
    format!("{:.6}", micros as f64 / 1_000_000.0)
}

fn escape_concat_path(path: &Path) -> Result<String, String> {
    let raw = path.to_str().ok_or("路径不是 UTF-8")?;
    Ok(format!("file '{}'", raw.replace('\'', "'\\''")))
}

/// 每段源信息（用于推导统一输出规格）。
struct SegmentInfo {
    has_audio: bool,
    display_width: u32,
    display_height: u32,
    fps_num: u32,
    fps_den: u32,
}

fn probe_segment_info(bins: &core::FfmpegBins, path: &Path) -> Result<SegmentInfo, String> {
    let out = std::process::Command::new(&bins.ffprobe)
        .args(["-v", "quiet", "-print_format", "json", "-show_streams"])
        .arg(path)
        .output()
        .map_err(|e| format!("ffprobe 调用失败：{e}"))?;
    if !out.status.success() {
        return Err(format!("ffprobe 解析失败：{}", path.display()));
    }
    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("ffprobe 输出解析失败：{e}"))?;
    let streams = json
        .get("streams")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut has_audio = false;
    let mut width = 0u32;
    let mut height = 0u32;
    let mut rotation = 0i32;
    let mut fps_num = 0u32;
    let mut fps_den = 0u32;
    for stream in &streams {
        match stream.get("codec_type").and_then(|v| v.as_str()) {
            Some("audio") => has_audio = true,
            Some("video") if width == 0 => {
                width = stream.get("width").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                height = stream.get("height").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                rotation = super::clip::rotation_of(stream);
                if let Some(raw) = stream.get("avg_frame_rate").and_then(|v| v.as_str()) {
                    if let Some((num, den)) = raw.split_once('/') {
                        if let (Ok(n), Ok(d)) = (num.parse::<u32>(), den.parse::<u32>()) {
                            if n > 0 && d > 0 {
                                fps_num = n;
                                fps_den = d;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if width == 0 || height == 0 {
        return Err(format!("未找到视频流：{}", path.display()));
    }
    let swapped = rotation.abs() % 180 == 90;
    let (display_width, display_height) = if swapped {
        (height, width)
    } else {
        (width, height)
    };
    Ok(SegmentInfo {
        has_audio,
        display_width,
        display_height,
        fps_num,
        fps_den,
    })
}

/// 统一输出规格：concat copy 要求各段流参数完全一致。
struct EncodeSpec {
    width: u32,
    height: u32,
    fps: Option<(u32, u32)>,
    with_audio: bool,
}

/// 推导统一输出规格：以第一段（应用 rotation 后）显示尺寸为基准，等比缩放 + 补边；
/// 帧率统一到第一段；音轨混合时整片去音频（concat copy 要求流结构一致）。
///
/// 内含逐段 ffprobe 子进程（同步阻塞），须经 `spawn_blocking` 调用。
fn derive_spec(
    bins: &core::FfmpegBins,
    segments: &[ClipSegmentRequest],
    target_height: u32,
) -> Result<EncodeSpec, String> {
    let mut infos = Vec::with_capacity(segments.len());
    for seg in segments {
        infos.push(probe_segment_info(bins, Path::new(&seg.path))?);
    }
    let first_info = &infos[0];
    let with_audio = infos.iter().all(|info| info.has_audio);
    let target_h = if target_height > 0 {
        target_height + (target_height & 1)
    } else {
        (first_info.display_height.max(2)) & !1
    };
    let ratio = first_info.display_width as f64 / first_info.display_height.max(1) as f64;
    let target_w = ((((target_h as f64) * ratio).round() as u32).max(2)) & !1;
    let fps = if first_info.fps_num > 0 && first_info.fps_den > 0 {
        Some((first_info.fps_num, first_info.fps_den))
    } else {
        None
    };
    Ok(EncodeSpec {
        width: target_w,
        height: target_h,
        fps,
        with_audio,
    })
}

/// 跑一段编码（解码区间 = [in, out)）并回报段内进度。
async fn encode_segment(
    bins: &core::FfmpegBins,
    seg: &ClipSegmentRequest,
    out_path: &Path,
    spec: &EncodeSpec,
    cancel: &CancellationToken,
    mut on_progress: impl FnMut(f64),
) -> Result<(), String> {
    if seg.out_us <= seg.in_us {
        return Err("片段时长无效".into());
    }
    let speed = clamp_speed(seg.speed);
    let mut cmd = tokio::process::Command::new(&bins.ffmpeg);
    cmd.args([
        "-hide_banner",
        "-nostdin",
        "-y",
        "-loglevel",
        "error",
        "-progress",
        "pipe:1",
    ])
    .arg("-ss")
    .arg(seconds(seg.in_us))
    .arg("-to")
    .arg(seconds(seg.out_us))
    .arg("-i")
    .arg(&seg.path)
    .args([
        "-c:v", "libx264", "-preset", "veryfast", "-crf", "18", "-pix_fmt", "yuv420p",
    ]);
    if spec.with_audio {
        if (speed - 1.0).abs() > f64::EPSILON {
            // 音频变速保音高（atempo 链覆盖 0.25–8）
            cmd.args(["-af", &atempo_chain(speed)]);
        }
        cmd.args(["-c:a", "aac", "-b:a", "192k", "-ar", "48000", "-ac", "2"]);
    } else {
        cmd.arg("-an");
    }
    // 统一到目标框：等比缩放 + 居中补边（混合横竖屏也能 concat）；
    // 倍速经 setpts 拉伸时间戳，`-r` 在其后按目标帧率重采样（2x 丢帧 / 0.5x 补帧，
    // 输出恒定帧率保证 concat 规格一致）
    let mut filter = format!(
        "scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,setsar=1",
        w = spec.width,
        h = spec.height
    );
    if (speed - 1.0).abs() > f64::EPSILON {
        filter.push_str(&format!(",setpts=PTS/{speed:.4}"));
    }
    cmd.args(["-vf", &filter]);
    if let Some((num, den)) = spec.fps {
        cmd.args(["-r", &format!("{num}/{den}")]);
    }
    cmd.args(["-movflags", "+faststart"]).arg(out_path);
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    let mut child = cmd.spawn().map_err(|e| format!("编码进程启动失败：{e}"))?;
    let stdout = child.stdout.take().ok_or("无法读取编码进度输出")?;
    let mut stderr = child.stderr.take().ok_or("无法读取编码错误输出")?;
    let stderr_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        let _ = stderr.read_to_end(&mut buf).await;
        buf
    });

    let total = (((seg.out_us - seg.in_us).max(1)) as f64 / speed)
        .round()
        .max(1.0);
    let mut lines = tokio::io::BufReader::new(stdout).lines();
    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                let _ = stderr_task.await;
                return Err("已取消".into());
            }
            line = lines.next_line() => {
                match line {
                    Ok(Some(line)) => {
                        if let Some(raw) = line.strip_prefix("out_time_us=") {
                            if let Ok(us) = raw.trim().parse::<i64>() {
                                if us >= 0 {
                                    on_progress((us as f64 / total).clamp(0.0, 1.0));
                                }
                            }
                        } else if line.starts_with("progress=end") {
                            on_progress(1.0);
                        }
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
        }
    }
    let status = child
        .wait()
        .await
        .map_err(|e| format!("编码进程等待失败：{e}"))?;
    let stderr_bytes = stderr_task.await.unwrap_or_default();
    if !status.success() {
        let text = String::from_utf8_lossy(&stderr_bytes);
        let tail: String = text.lines().rev().take(4).collect::<Vec<_>>().join(" / ");
        return Err(if tail.is_empty() {
            "编码失败".into()
        } else {
            format!("编码失败：{tail}")
        });
    }
    if !out_path.is_file() || std::fs::metadata(out_path).map(|m| m.len()).unwrap_or(0) == 0 {
        return Err("编码输出为空".into());
    }
    Ok(())
}

/// 单段快速导出：输入侧 `-ss` + `-c copy`（mp4/mov 保留 edit list，画面精确到请求帧）。
async fn copy_segment(
    bins: &core::FfmpegBins,
    seg: &ClipSegmentRequest,
    out_path: &Path,
    cancel: &CancellationToken,
) -> Result<(), String> {
    let mut cmd = tokio::process::Command::new(&bins.ffmpeg);
    cmd.args(["-hide_banner", "-nostdin", "-y", "-loglevel", "error"])
        .arg("-ss")
        .arg(seconds(seg.in_us))
        .arg("-to")
        .arg(seconds(seg.out_us))
        .arg("-i")
        .arg(&seg.path)
        .args(["-c", "copy", "-movflags", "+faststart"])
        .arg(out_path)
        .kill_on_drop(true);
    let mut child = cmd.spawn().map_err(|e| format!("导出进程启动失败：{e}"))?;
    let status = tokio::select! {
        biased;
        _ = cancel.cancelled() => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err("已取消".into());
        }
        status = child.wait() => status.map_err(|e| format!("导出进程等待失败：{e}"))?,
    };
    if !status.success() {
        return Err("快速导出失败（源文件可能不是 mp4/mov）".into());
    }
    if !out_path.is_file() || std::fs::metadata(out_path).map(|m| m.len()).unwrap_or(0) == 0 {
        return Err("导出输出为空".into());
    }
    Ok(())
}

async fn write_concat_list(segments: &[PathBuf], list_path: &Path) -> Result<(), String> {
    let mut text = String::new();
    for path in segments {
        text.push_str(&escape_concat_path(path)?);
        text.push('\n');
    }
    tokio::fs::write(list_path, text)
        .await
        .map_err(|e| format!("拼接清单写入失败：{e}"))
}

/// 启动导出（单任务，新增任务前需等待上一个结束）。
///
/// 运行期错误只经 `Channel` 的 `Error` 事件回报、命令本身返回 `Ok`（与 agent_run 同
/// 范式）——否则前端 channel 回调与 invoke reject 各弹一次 toast，错误会双弹。
#[tauri::command]
pub async fn video_clip_export(
    app: AppHandle,
    request: ClipExportRequest,
    on_event: Channel<ClipExportEvent>,
) -> Result<(), String> {
    if request.segments.is_empty() {
        return Err("没有可导出的片段".into());
    }
    if EXPORT_BUSY
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("已有导出任务在进行".into());
    }
    let cancel = CancellationToken::new();
    if let Ok(mut slot) = cancel_slot().lock() {
        *slot = Some(cancel.clone());
    }
    let result = run_export(&app, request, &on_event, &cancel).await;
    if let Ok(mut slot) = cancel_slot().lock() {
        *slot = None;
    }
    EXPORT_BUSY.store(false, Ordering::SeqCst);
    if let Err(message) = &result {
        emit(
            &on_event,
            ClipExportEvent::Error {
                message: message.clone(),
            },
        );
    }
    Ok(())
}

async fn run_export(
    app: &AppHandle,
    request: ClipExportRequest,
    on_event: &Channel<ClipExportEvent>,
    cancel: &CancellationToken,
) -> Result<(), String> {
    let bins = tauri::async_runtime::spawn_blocking({
        let app = app.clone();
        move || core::resolve_bins(&app).ok_or("FFmpeg 未就绪，请先下载核心".to_string())
    })
    .await
    .map_err(|e| e.to_string())??;

    // 校验全部源文件
    for seg in &request.segments {
        if !crate::platform::path_guard::validate(Path::new(&seg.path)) {
            return Err(format!("路径不安全或不存在：{}", seg.path));
        }
        if seg.out_us <= seg.in_us {
            return Err("片段时长无效".into());
        }
    }

    // 成片总长（虚拟时长 = Σ 源时长/倍速）：进度与 Done 回报的单一真相源
    let total_us: i64 = request
        .segments
        .iter()
        .map(|s| {
            (((s.out_us - s.in_us).max(1)) as f64 / clamp_speed(s.speed))
                .round()
                .max(1.0) as i64
        })
        .sum();
    let first = Path::new(&request.segments[0].path);
    let output_path = build_output_path(first, request.output_path.as_deref())?;

    // 输出目录存在性（显式指定时）
    if let Some(parent) = output_path.parent() {
        if !parent.is_dir() {
            return Err("输出目录不存在".into());
        }
    }

    emit(
        on_event,
        ClipExportEvent::Started {
            total_us,
            segments: request.segments.len(),
        },
    );

    let work_dir = temp_export_dir().join(format!(
        "export-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&work_dir).map_err(|e| format!("临时目录创建失败：{e}"))?;

    let cleanup = |dir: &Path| {
        let _ = std::fs::remove_dir_all(dir);
    };

    // ── 快速档：单段 copy ──
    if request.mode == "copy" {
        if request.segments.len() > 1 {
            cleanup(&work_dir);
            return Err("快速导出仅支持单段（多段拼接需精确导出）".into());
        }
        // copy 流拷贝无法变速（setpts/atempo 都要求重编码），静默忽略会产出错误时长
        if (clamp_speed(request.segments[0].speed) - 1.0).abs() > f64::EPSILON {
            cleanup(&work_dir);
            return Err("快速导出不支持倍速（倍速需精确导出）".into());
        }
        emit(
            on_event,
            ClipExportEvent::Progress {
                percent: 5.0,
                stage: "copy".into(),
            },
        );
        let result = copy_segment(&bins, &request.segments[0], &output_path, cancel).await;
        return match result {
            Ok(()) => {
                cleanup(&work_dir);
                let size_bytes = std::fs::metadata(&output_path)
                    .map(|m| m.len())
                    .unwrap_or(0);
                emit(
                    on_event,
                    ClipExportEvent::Done {
                        output_path: output_path.to_string_lossy().to_string(),
                        size_bytes,
                        duration_us: total_us,
                    },
                );
                Ok(())
            }
            Err(err) => {
                cleanup(&work_dir);
                let _ = std::fs::remove_file(&output_path);
                Err(err)
            }
        };
    }

    // ── 精确档：逐段重编码 → concat copy ──
    // 逐段 ffprobe（同步子进程）放 blocking 线程，不占 tokio worker
    let spec = tauri::async_runtime::spawn_blocking({
        let bins = bins.clone();
        let segments = request.segments.clone();
        let target_height = request.target_height;
        move || derive_spec(&bins, &segments, target_height)
    })
    .await
    .map_err(|e| e.to_string())??;
    let mut encoded: Vec<PathBuf> = Vec::with_capacity(request.segments.len());
    let mut completed_us: i64 = 0;

    for (index, seg) in request.segments.iter().enumerate() {
        if cancel.is_cancelled() {
            cleanup(&work_dir);
            return Err("已取消".into());
        }
        let seg_path = work_dir.join(format!("seg-{index}.mp4"));
        // 段进度占比按虚拟时长（与 out_time 同侧）
        let seg_total = (((seg.out_us - seg.in_us).max(1)) as f64 / clamp_speed(seg.speed))
            .round()
            .max(1.0);
        let completed_before = completed_us;
        let stage = format!("segment {}/{}", index + 1, request.segments.len());
        let percent_base = completed_before as f64 / total_us.max(1) as f64 * 95.0;
        let percent_span = seg_total / total_us.max(1) as f64 * 95.0;
        let result = encode_segment(&bins, seg, &seg_path, &spec, cancel, |fraction| {
            emit(
                on_event,
                ClipExportEvent::Progress {
                    percent: (percent_base + fraction * percent_span).clamp(0.0, 99.0),
                    stage: stage.clone(),
                },
            );
        })
        .await;
        if let Err(err) = result {
            cleanup(&work_dir);
            return Err(err);
        }
        encoded.push(seg_path);
        completed_us += seg.out_us - seg.in_us;
    }

    // concat：逐段同规格 → `-c copy` 合并
    emit(
        on_event,
        ClipExportEvent::Progress {
            percent: 97.0,
            stage: "concat".into(),
        },
    );
    let list_path = work_dir.join("concat.txt");
    write_concat_list(&encoded, &list_path).await?;
    let mut cmd = tokio::process::Command::new(&bins.ffmpeg);
    cmd.args(["-hide_banner", "-nostdin", "-y", "-loglevel", "error"])
        .args(["-f", "concat", "-safe", "0", "-i"])
        .arg(&list_path)
        .args(["-c", "copy", "-movflags", "+faststart"])
        .arg(&output_path)
        .kill_on_drop(true);
    let output = tokio::select! {
        biased;
        _ = cancel.cancelled() => {
            cleanup(&work_dir);
            return Err("已取消".into());
        }
        output = cmd.output() => output.map_err(|e| format!("拼接进程失败：{e}"))?,
    };
    if !output.status.success() {
        cleanup(&work_dir);
        let err = String::from_utf8_lossy(&output.stderr);
        let _ = std::fs::remove_file(&output_path);
        return Err(format!("拼接失败：{}", err.trim()));
    }
    cleanup(&work_dir);
    if !output_path.is_file() {
        return Err("导出输出不存在".into());
    }
    let size_bytes = std::fs::metadata(&output_path)
        .map(|m| m.len())
        .unwrap_or(0);
    emit(
        on_event,
        ClipExportEvent::Done {
            output_path: output_path.to_string_lossy().to_string(),
            size_bytes,
            duration_us: total_us,
        },
    );
    Ok(())
}

/// 取消进行中的导出（无任务时 no-op）。返回是否存在被取消的任务。
///
/// 双入口：`video_clip_cancel` 命令 + 剪辑窗口 Destroyed 钩子（窗口关闭时前端
/// onBeforeUnmount 随 webview 销毁不可靠执行，Rust 侧是权威取消点）。
pub(crate) fn cancel_export() -> bool {
    if let Ok(slot) = cancel_slot().lock() {
        if let Some(token) = slot.as_ref() {
            token.cancel();
            return true;
        }
    }
    false
}

/// 取消正在进行的导出。
#[tauri::command]
pub async fn video_clip_cancel() -> Result<bool, String> {
    Ok(cancel_export())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atempo_covers_out_of_range_speeds() {
        assert_eq!(atempo_chain(1.5), "atempo=1.5000");
        assert_eq!(atempo_chain(4.0), "atempo=2.0,atempo=2.0000");
        assert_eq!(atempo_chain(0.25), "atempo=0.5,atempo=0.5000");
        assert_eq!(atempo_chain(3.0), "atempo=2.0,atempo=1.5000");
    }

    #[test]
    fn speed_clamp() {
        assert_eq!(clamp_speed(0.0), 1.0);
        assert_eq!(clamp_speed(f64::NAN), 1.0);
        assert_eq!(clamp_speed(0.1), 0.25);
        assert_eq!(clamp_speed(100.0), 8.0);
        assert_eq!(clamp_speed(2.0), 2.0);
    }

    #[test]
    fn seconds_are_microsecond_precise() {
        assert_eq!(seconds(0), "0.000000");
        assert_eq!(seconds(1_000_000), "1.000000");
        assert_eq!(seconds(3_400_000), "3.400000");
        assert_eq!(seconds(8_333_000), "8.333000");
    }

    #[test]
    fn concat_escapes_single_quote() {
        let p = Path::new("/tmp/a'b.mp4");
        assert_eq!(escape_concat_path(p).unwrap(), "file '/tmp/a'\\''b.mp4'");
    }

    #[test]
    fn output_path_defaults_next_to_source() {
        let dir = std::env::temp_dir().join("voidnix-test-clip-export");
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("demo.mp4");
        std::fs::write(&src, b"x").unwrap();
        let out = build_output_path(&src, None).unwrap();
        assert_eq!(out, dir.join("demo-cut.mp4"));
        std::fs::write(dir.join("demo-cut.mp4"), b"x").unwrap();
        let out2 = build_output_path(&src, None).unwrap();
        assert_eq!(out2, dir.join("demo-cut-1.mp4"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
