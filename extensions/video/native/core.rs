//! FFmpeg / ffprobe 解析与按需下载（对齐 proxy ensure_bin 模式）。

use crate::runtime::storage::ext_data_dir;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::LazyLock;
use tauri::{AppHandle, Emitter};

/// 静态构建版本标签（eugeneware/ffmpeg-static）。
const FFMPEG_STATIC_TAG: &str = "b6.1.1";
const RELEASE_BASE: &str = "https://github.com/eugeneware/ffmpeg-static/releases/download";
/// 国内镜像前缀（与 proxy 同策略）。
const MIRROR_PREFIX: &str = "https://gh-proxy.com/";

const SHA256_FFMPEG_ARM64: &str =
    "8923876afa8db5585022d7860ec7e589af192f441c56793971276d450ed3bbfa";
const SHA256_FFMPEG_X64: &str = "929b375c1182d956c51f7ac25e0b2b0411fb01f6f407aa15c9758efeb4242106";
const SHA256_FFPROBE_ARM64: &str =
    "d986a8ec7b030899fe66a8a288ed809a3543338705a3ce178cfb85869c5d80be";
const SHA256_FFPROBE_X64: &str = "d4da574d6e2e197bd259b47d69cf262df9e312af24ad960444f6d806d3d4c186";

static DOWNLOADING: AtomicBool = AtomicBool::new(false);
static DOWNLOAD_LOCK: LazyLock<tokio::sync::Mutex<()>> =
    LazyLock::new(|| tokio::sync::Mutex::new(()));

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreStatus {
    /// 是否可调用（系统或已下载）。
    pub available: bool,
    /// `path` | `bundled` | `none`
    pub source: String,
    pub version: String,
    pub downloading: bool,
}

/// 已解析的 ffmpeg / ffprobe 可执行路径。
#[derive(Clone, Debug)]
pub struct FfmpegBins {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub source: &'static str,
}

/// ffmpeg-static asset 架构名（arm64 / x64）。
fn ffmpeg_arch_slug() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x64"
    }
}

fn bundled_ffmpeg(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(ext_data_dir(app, "video")?.join("ffmpeg"))
}

fn bundled_ffprobe(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(ext_data_dir(app, "video")?.join("ffprobe"))
}

/// 是否为可执行文件（存在 + Unix 可执行位）。
fn is_executable_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// 在 PATH 中查找可执行文件。
fn find_on_path(name: &str) -> Option<PathBuf> {
    let Ok(path_var) = std::env::var("PATH") else {
        return None;
    };
    for dir in path_var.split(':') {
        if dir.is_empty() {
            continue;
        }
        let candidate = Path::new(dir).join(name);
        if is_executable_file(&candidate) {
            return Some(candidate);
        }
    }
    // Homebrew 常见路径兜底（GUI app PATH 常缺）
    for prefix in ["/opt/homebrew/bin", "/usr/local/bin"] {
        let candidate = Path::new(prefix).join(name);
        if is_executable_file(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn version_of(bin: &Path) -> String {
    let out = Command::new(bin).arg("-version").output();
    match out {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout);
            // "ffmpeg version N.M.P ..." / "ffprobe version ..."
            text.lines()
                .next()
                .and_then(|l| {
                    l.split_whitespace()
                        .nth(2)
                        .map(|v| v.trim_end_matches(',').to_string())
                })
                .unwrap_or_default()
        }
        _ => String::new(),
    }
}

/// 解析可用的 ffmpeg/ffprobe（不触发下载）。
pub fn resolve_bins(app: &AppHandle) -> Option<FfmpegBins> {
    if let (Some(ffmpeg), Some(ffprobe)) = (find_on_path("ffmpeg"), find_on_path("ffprobe")) {
        return Some(FfmpegBins {
            ffmpeg,
            ffprobe,
            source: "path",
        });
    }
    let Ok(ffmpeg) = bundled_ffmpeg(app) else {
        return None;
    };
    let Ok(ffprobe) = bundled_ffprobe(app) else {
        return None;
    };
    if ffmpeg.is_file() && ffprobe.is_file() {
        return Some(FfmpegBins {
            ffmpeg,
            ffprobe,
            source: "bundled",
        });
    }
    None
}

pub fn core_status(app: &AppHandle) -> CoreStatus {
    let downloading = DOWNLOADING.load(Ordering::Relaxed);
    match resolve_bins(app) {
        Some(bins) => CoreStatus {
            available: true,
            source: bins.source.into(),
            version: version_of(&bins.ffmpeg),
            downloading,
        },
        None => CoreStatus {
            available: false,
            source: "none".into(),
            version: String::new(),
            downloading,
        },
    }
}

/// 确保 bin 就绪：系统/已下载直接返回，否则下载。
pub async fn ensure_bins(app: &AppHandle) -> Result<FfmpegBins, String> {
    if let Some(bins) = resolve_bins(app) {
        return Ok(bins);
    }
    let _guard = DOWNLOAD_LOCK.lock().await;
    if let Some(bins) = resolve_bins(app) {
        return Ok(bins);
    }
    download_static(app).await?;
    resolve_bins(app).ok_or_else(|| "FFmpeg 下载完成但无法解析路径".into())
}

async fn download_static(app: &AppHandle) -> Result<(), String> {
    let arch = ffmpeg_arch_slug();
    let dir = ext_data_dir(app, "video")?;
    let (ffmpeg_sha, ffprobe_sha) = if arch == "arm64" {
        (SHA256_FFMPEG_ARM64, SHA256_FFPROBE_ARM64)
    } else {
        (SHA256_FFMPEG_X64, SHA256_FFPROBE_X64)
    };

    DOWNLOADING.store(true, Ordering::Relaxed);
    let result = async {
        use crate::runtime::binary_fetch::{fetch, BinaryFetch};

        // progress_base 累计两段下载，避免第二段进度从 0 回跳
        let mut progress_base = 0u64;
        let ffmpeg_gz = dir.join("ffmpeg.gz");
        let ffmpeg_bin = dir.join("ffmpeg");
        let ffmpeg_url = format!("{RELEASE_BASE}/{FFMPEG_STATIC_TAG}/ffmpeg-darwin-{arch}.gz");
        progress_base += file_size_or_zero(&ffmpeg_gz);
        fetch(
            app,
            BinaryFetch {
                urls: vec![format!("{MIRROR_PREFIX}{ffmpeg_url}"), ffmpeg_url],
                gz_path: &ffmpeg_gz,
                bin_path: &ffmpeg_bin,
                expected_sha256: ffmpeg_sha,
                progress_event: "video-core-progress",
                progress_base,
            },
        )
        .await?;

        let ffprobe_gz = dir.join("ffprobe.gz");
        let ffprobe_bin = dir.join("ffprobe");
        let ffprobe_url = format!("{RELEASE_BASE}/{FFMPEG_STATIC_TAG}/ffprobe-darwin-{arch}.gz");
        progress_base += file_size_or_zero(&ffprobe_gz);
        fetch(
            app,
            BinaryFetch {
                urls: vec![format!("{MIRROR_PREFIX}{ffprobe_url}"), ffprobe_url],
                gz_path: &ffprobe_gz,
                bin_path: &ffprobe_bin,
                expected_sha256: ffprobe_sha,
                progress_event: "video-core-progress",
                progress_base,
            },
        )
        .await?;

        let _ = std::fs::write(dir.join("ffmpeg.version"), FFMPEG_STATIC_TAG);
        let _ = app.emit("video-core-ready", ());
        Ok::<(), String>(())
    }
    .await;
    DOWNLOADING.store(false, Ordering::Relaxed);
    result
}

/// 文件存在时的字节数（不存在返 0），供 progress_base 累计偏移估算。
fn file_size_or_zero(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// ffprobe 结构化元数据。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoMeta {
    pub path: String,
    pub duration_secs: f64,
    pub width: u32,
    pub height: u32,
    pub video_codec: String,
    pub audio_codec: String,
    pub size_bytes: u64,
    pub container: String,
}

pub fn probe(app: &AppHandle, path: &str) -> Result<VideoMeta, String> {
    let bins = resolve_bins(app).ok_or("FFmpeg 未就绪，请先下载内核")?;
    let p = Path::new(path);
    if !crate::platform::path_guard::validate(p) {
        return Err("路径不安全或不存在".into());
    }
    let size_bytes = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);

    let out = Command::new(&bins.ffprobe)
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            path,
        ])
        .output()
        .map_err(|e| format!("ffprobe 调用失败: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("ffprobe 失败: {err}"));
    }
    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("ffprobe JSON 解析失败: {e}"))?;

    let duration_secs = json
        .pointer("/format/duration")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .or_else(|| json.pointer("/format/duration").and_then(|v| v.as_f64()))
        .unwrap_or(0.0);

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

    let mut width = 0u32;
    let mut height = 0u32;
    let mut video_codec = String::new();
    let mut audio_codec = String::new();
    for s in &streams {
        let codec_type = s.get("codec_type").and_then(|v| v.as_str()).unwrap_or("");
        let codec = s
            .get("codec_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if codec_type == "video" && video_codec.is_empty() {
            video_codec = codec;
            width = s.get("width").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            height = s.get("height").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        } else if codec_type == "audio" && audio_codec.is_empty() {
            audio_codec = codec;
        }
    }

    Ok(VideoMeta {
        path: path.to_string(),
        duration_secs,
        width,
        height,
        video_codec,
        audio_codec,
        size_bytes,
        container,
    })
}
