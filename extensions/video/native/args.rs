//! 预设 → ffmpeg argv 构造（纯逻辑，可单测）。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VideoMode {
    Compress,
    Convert,
    ExtractAudio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Quality {
    High,
    Balanced,
    Small,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scale {
    Original,
    #[serde(rename = "1080")]
    P1080,
    #[serde(rename = "720")]
    P720,
    #[serde(rename = "480")]
    P480,
}

impl Scale {
    pub fn height(self) -> Option<u32> {
        match self {
            Scale::Original => None,
            Scale::P1080 => Some(1080),
            Scale::P720 => Some(720),
            Scale::P480 => Some(480),
        }
    }
}

/// 输出容器/编码目标扩展名（无点号）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Mp4,
    Mov,
    Mkv,
    Webm,
    Gif,
    M4a,
    Mp3,
}

impl OutputFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            OutputFormat::Mp4 => "mp4",
            OutputFormat::Mov => "mov",
            OutputFormat::Mkv => "mkv",
            OutputFormat::Webm => "webm",
            OutputFormat::Gif => "gif",
            OutputFormat::M4a => "m4a",
            OutputFormat::Mp3 => "mp3",
        }
    }

    pub fn is_audio_only(self) -> bool {
        matches!(self, OutputFormat::M4a | OutputFormat::Mp3)
    }

    pub fn is_gif(self) -> bool {
        matches!(self, OutputFormat::Gif)
    }
}

/// 转码参数（前端传入）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncodeParams {
    pub mode: VideoMode,
    pub format: OutputFormat,
    pub quality: Quality,
    pub scale: Scale,
    /// 优先 VideoToolbox 硬件编码。
    #[serde(default = "default_true")]
    pub prefer_hardware: bool,
}

fn default_true() -> bool {
    true
}

/// 构造 ffmpeg 参数（不含可执行文件本身）。
/// `input` / `output` 为绝对路径字符串。
///
/// 返回 (args, uses_hardware)。若 hardware 路径失败，调用方可设 prefer_hardware=false 重试。
pub fn build_ffmpeg_args(
    input: &str,
    output: &str,
    params: &EncodeParams,
    force_software: bool,
) -> (Vec<String>, bool) {
    let mut args: Vec<String> = vec![
        "-hide_banner".into(),
        "-y".into(),
        "-i".into(),
        input.into(),
        "-progress".into(),
        "pipe:1".into(),
        "-nostats".into(),
    ];

    let use_hw = params.prefer_hardware
        && !force_software
        && !params.format.is_audio_only()
        && !params.format.is_gif()
        && !matches!(params.format, OutputFormat::Webm);

    // 缩放滤镜
    let scale_h = params.scale.height();
    let mut vf_parts: Vec<String> = Vec::new();
    if let Some(h) = scale_h {
        // 偶宽 + 高度上限，不放大
        vf_parts.push(format!("scale=-2:'min({h},ih)'"));
    }

    match params.mode {
        VideoMode::ExtractAudio => {
            args.push("-vn".into());
            match params.format {
                OutputFormat::Mp3 => {
                    args.extend(["-c:a".into(), "libmp3lame".into()]);
                    args.extend(audio_bitrate(params.quality, false));
                }
                _ => {
                    // m4a 默认
                    args.extend(["-c:a".into(), "aac".into()]);
                    args.extend(audio_bitrate(params.quality, false));
                }
            }
        }
        _ if params.format.is_gif() => {
            // 两遍 palette 简化为单滤镜链（palettegen+paletteuse）
            let fps = match params.quality {
                Quality::High => 15,
                Quality::Balanced => 12,
                Quality::Small => 8,
            };
            let mut gif_vf = format!("fps={fps}");
            if let Some(h) = scale_h.or(Some(480)) {
                gif_vf.push_str(&format!(",scale=-2:'min({h},ih)':flags=lanczos"));
            }
            gif_vf.push_str(",split[s0][s1];[s0]palettegen=max_colors=128[p];[s1][p]paletteuse");
            args.extend(["-vf".into(), gif_vf]);
            args.extend(["-loop".into(), "0".into()]);
        }
        _ if params.format == OutputFormat::Webm => {
            args.extend(["-c:v".into(), "libvpx-vp9".into()]);
            args.extend(vp9_quality(params.quality));
            args.extend(["-c:a".into(), "libopus".into()]);
            args.extend(audio_bitrate(params.quality, false));
            if !vf_parts.is_empty() {
                args.extend(["-vf".into(), vf_parts.join(",")]);
            }
        }
        _ => {
            // mp4 / mov / mkv：压缩与转换参数分离
            // 压缩必须明显缩小体积；转换偏画质保真
            let compress = params.mode == VideoMode::Compress;
            if use_hw {
                args.extend(["-c:v".into(), "h264_videotoolbox".into()]);
                if compress {
                    // VT 的 -q:v 对已压片源常「越压越大」；改平均码率封顶
                    args.extend(vt_compress_bitrate(params.quality, params.scale));
                } else {
                    args.extend(vt_quality(params.quality));
                }
            } else {
                args.extend(["-c:v".into(), "libx264".into()]);
                args.extend(x264_quality(params.quality, compress));
            }
            args.extend(["-c:a".into(), "aac".into()]);
            args.extend(audio_bitrate(params.quality, compress));
            // 兼容 QuickTime / 多数播放器（scale 等滤镜后易出非 4:2:0）
            args.extend(["-pix_fmt".into(), "yuv420p".into()]);
            if !vf_parts.is_empty() {
                args.extend(["-vf".into(), vf_parts.join(",")]);
            }
            if matches!(params.format, OutputFormat::Mp4 | OutputFormat::Mov) {
                args.extend(["-movflags".into(), "+faststart".into()]);
            }
        }
    }

    args.push(output.into());
    (args, use_hw)
}

fn vt_quality(q: Quality) -> Vec<String> {
    // 转换模式：VideoToolbox q:v 约 1–100，数值越高质量越好
    let qv = match q {
        Quality::High => "65",
        Quality::Balanced => "50",
        Quality::Small => "35",
    };
    vec!["-q:v".into(), qv.into()]
}

/// 压缩模式硬件编码：用平均码率封顶，避免 q:v 把已压 H.264 撑大。
/// 码率按 1080p 基准，随 scale 高度线性缩放（original 按 1080 计）。
fn vt_compress_bitrate(q: Quality, scale: Scale) -> Vec<String> {
    let base_kbps: u32 = match q {
        Quality::High => 2500,
        Quality::Balanced => 1200,
        Quality::Small => 700,
    };
    let h = scale.height().unwrap_or(1080) as f64;
    let kbps = ((base_kbps as f64) * (h / 1080.0))
        .round()
        .clamp(400.0, 6000.0) as u32;
    let max = (kbps * 12) / 10;
    let buf = max * 2;
    vec![
        "-b:v".into(),
        format!("{kbps}k"),
        "-maxrate".into(),
        format!("{max}k"),
        "-bufsize".into(),
        format!("{buf}k"),
    ]
}

fn x264_quality(q: Quality, compress: bool) -> Vec<String> {
    // 压缩用更高 CRF（体积优先）；转换保画质
    let (crf, preset) = if compress {
        match q {
            Quality::High => ("26", "medium"),
            Quality::Balanced => ("30", "medium"),
            Quality::Small => ("34", "fast"),
        }
    } else {
        match q {
            Quality::High => ("20", "slow"),
            Quality::Balanced => ("23", "medium"),
            Quality::Small => ("28", "fast"),
        }
    };
    vec!["-crf".into(), crf.into(), "-preset".into(), preset.into()]
}

fn vp9_quality(q: Quality) -> Vec<String> {
    let crf = match q {
        Quality::High => "28",
        Quality::Balanced => "33",
        Quality::Small => "38",
    };
    vec![
        "-b:v".into(),
        "0".into(),
        "-crf".into(),
        crf.into(),
        "-row-mt".into(),
        "1".into(),
    ]
}

fn audio_bitrate(q: Quality, compress: bool) -> Vec<String> {
    let br = if compress {
        match q {
            Quality::High => "128k",
            Quality::Balanced => "96k",
            Quality::Small => "64k",
        }
    } else {
        match q {
            Quality::High => "192k",
            Quality::Balanced => "128k",
            Quality::Small => "96k",
        }
    };
    vec!["-b:a".into(), br.into()]
}

/// 输出文件 stem 后缀（动作标签）。
pub fn action_label(mode: VideoMode, _format: OutputFormat) -> &'static str {
    match mode {
        VideoMode::ExtractAudio => "audio",
        VideoMode::Compress => "compressed",
        VideoMode::Convert => "converted",
    }
}

/// 根据模式解析最终扩展名。
pub fn resolve_format(mode: VideoMode, format: OutputFormat) -> OutputFormat {
    match mode {
        VideoMode::ExtractAudio => {
            if format.is_audio_only() {
                format
            } else {
                OutputFormat::M4a
            }
        }
        VideoMode::Compress => {
            // 压缩默认容器 mp4
            if format.is_audio_only() || format.is_gif() {
                OutputFormat::Mp4
            } else {
                format
            }
        }
        VideoMode::Convert => format,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(mode: VideoMode, format: OutputFormat) -> EncodeParams {
        EncodeParams {
            mode,
            format,
            quality: Quality::Balanced,
            scale: Scale::Original,
            prefer_hardware: true,
        }
    }

    #[test]
    fn compress_uses_videotoolbox_bitrate_not_qv() {
        let (args, hw) = build_ffmpeg_args(
            "/in.mp4",
            "/out.mp4",
            &params(VideoMode::Compress, OutputFormat::Mp4),
            false,
        );
        assert!(hw);
        assert!(args.iter().any(|a| a == "h264_videotoolbox"));
        assert!(args.iter().any(|a| a == "-b:v"));
        assert!(!args.iter().any(|a| a == "-q:v"));
        assert!(args.iter().any(|a| a == "+faststart"));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "-pix_fmt" && w[1] == "yuv420p"));
        // 均衡默认 1200k
        assert!(args.iter().any(|a| a == "1200k"));
    }

    #[test]
    fn force_software_compress_uses_higher_crf() {
        let (args, hw) = build_ffmpeg_args(
            "/in.mp4",
            "/out.mp4",
            &params(VideoMode::Compress, OutputFormat::Mp4),
            true,
        );
        assert!(!hw);
        assert!(args.iter().any(|a| a == "libx264"));
        let crf = args
            .windows(2)
            .find(|w| w[0] == "-crf")
            .map(|w| w[1].as_str());
        assert_eq!(crf, Some("30"));
    }

    #[test]
    fn convert_keeps_quality_qv_on_vt() {
        let (args, hw) = build_ffmpeg_args(
            "/in.mp4",
            "/out.mp4",
            &params(VideoMode::Convert, OutputFormat::Mp4),
            false,
        );
        assert!(hw);
        assert!(args.iter().any(|a| a == "-q:v"));
    }

    #[test]
    fn extract_audio_no_video() {
        let (args, _) = build_ffmpeg_args(
            "/in.mp4",
            "/out.m4a",
            &params(VideoMode::ExtractAudio, OutputFormat::M4a),
            false,
        );
        assert!(args.iter().any(|a| a == "-vn"));
        assert!(args.iter().any(|a| a == "aac"));
    }

    #[test]
    fn gif_has_palette_filter() {
        let (args, hw) = build_ffmpeg_args(
            "/in.mp4",
            "/out.gif",
            &params(VideoMode::Convert, OutputFormat::Gif),
            false,
        );
        assert!(!hw);
        let vf = args
            .windows(2)
            .find(|w| w[0] == "-vf")
            .map(|w| w[1].as_str());
        assert!(vf.unwrap_or("").contains("palettegen"));
    }

    #[test]
    fn scale_filter_applied() {
        let mut p = params(VideoMode::Compress, OutputFormat::Mp4);
        p.scale = Scale::P720;
        let (args, _) = build_ffmpeg_args("/in.mp4", "/out.mp4", &p, true);
        let vf = args
            .windows(2)
            .find(|w| w[0] == "-vf")
            .map(|w| w[1].as_str());
        assert!(vf.unwrap_or("").contains("720"));
    }

    #[test]
    fn action_labels() {
        assert_eq!(
            action_label(VideoMode::Compress, OutputFormat::Mp4),
            "compressed"
        );
        assert_eq!(
            action_label(VideoMode::ExtractAudio, OutputFormat::M4a),
            "audio"
        );
        assert_eq!(
            action_label(VideoMode::Convert, OutputFormat::Gif),
            "converted"
        );
    }

    #[test]
    fn resolve_format_extract_defaults_m4a() {
        assert_eq!(
            resolve_format(VideoMode::ExtractAudio, OutputFormat::Mp4),
            OutputFormat::M4a
        );
    }
}
