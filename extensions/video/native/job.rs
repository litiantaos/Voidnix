//! 转码任务：单任务队列 + 进度解析 + 取消。

use super::args::{
    action_label, build_ffmpeg_args, resolve_format, EncodeParams, OutputFormat, VideoMode,
};
use super::core::{self, FfmpegBins};
use crate::runtime::lock_or_recover;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command as TokioCommand;
use tokio_util::sync::CancellationToken;

const JOB_TIMEOUT: Duration = Duration::from_secs(6 * 3600);

static BUSY: AtomicBool = AtomicBool::new(false);
static CANCEL: Mutex<Option<CancellationToken>> = Mutex::new(None);
static LAST_RESULT: Mutex<Option<JobSnapshot>> = Mutex::new(None);

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobSnapshot {
    pub busy: bool,
    pub last_output: Option<String>,
    pub last_error: Option<String>,
    pub last_percent: f64,
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum VideoEvent {
    #[serde(rename = "started")]
    Started { output_path: String },
    #[serde(rename = "progress")]
    Progress {
        percent: f64,
        time_secs: f64,
        speed: String,
    },
    #[serde(rename = "done")]
    Done {
        output_path: String,
        size_bytes: u64,
    },
    #[serde(rename = "error")]
    Error { message: String },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRequest {
    pub input_path: String,
    pub output_dir: Option<String>,
    pub params: EncodeParams,
    /// 探测得到的时长（秒），用于进度百分比；0 则只报 time。
    #[serde(default)]
    pub duration_secs: f64,
}

pub fn job_status() -> JobSnapshot {
    let last = LAST_RESULT
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or(JobSnapshot {
            busy: false,
            last_output: None,
            last_error: None,
            last_percent: 0.0,
        });
    JobSnapshot {
        busy: BUSY.load(Ordering::Relaxed),
        last_output: last.last_output,
        last_error: last.last_error,
        last_percent: last.last_percent,
    }
}

pub fn cancel_job() -> bool {
    let guard = lock_or_recover(&CANCEL);
    if let Some(token) = guard.as_ref() {
        token.cancel();
        return true;
    }
    false
}

fn set_last(snap: JobSnapshot) {
    *lock_or_recover(&LAST_RESULT) = Some(snap);
}

/// Channel + 全局事件双投递（重开面板 / 本地 Channel 共用终态契约）。
fn emit_both(app: &AppHandle, on_event: &Channel<VideoEvent>, ev: VideoEvent) {
    let _ = on_event.send(ev.clone());
    let _ = app.emit("video-job-event", &ev);
}

/// 校验输出目录安全后确保存在：向上找最近已存在祖先 → validate → create_dir_all。
fn ensure_output_dir(dir: &Path) -> Result<(), String> {
    let mut probe = dir.to_path_buf();
    loop {
        if probe.exists() {
            if !crate::platform::path_guard::validate(&probe) {
                return Err("输出目录不安全".into());
            }
            break;
        }
        match probe.parent() {
            Some(p) if p != probe.as_path() => probe = p.to_path_buf(),
            _ => return Err("输出目录不安全".into()),
        }
    }
    if !dir.exists() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建输出目录失败: {e}"))?;
        if !crate::platform::path_guard::validate(dir) {
            let _ = std::fs::remove_dir(dir);
            return Err("输出目录不安全".into());
        }
    }
    Ok(())
}

/// 净化文件名片段（去控制字符与路径分隔）。
fn sanitize_stem(stem: &str) -> String {
    stem.chars()
        .map(|c| {
            if c.is_control() || c == '/' || c == '\\' || c == '\0' {
                '_'
            } else {
                c
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// 生成不冲突的输出路径：`{stem}.{action}.{ext}`，冲突则追加 -1/-2。
pub fn build_output_path(
    input: &Path,
    output_dir: Option<&Path>,
    mode: VideoMode,
    format: OutputFormat,
) -> Result<PathBuf, String> {
    let format = resolve_format(mode, format);
    let dir = match output_dir {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => input
            .parent()
            .map(|p| p.to_path_buf())
            .ok_or("无法解析输出目录")?,
    };
    // 先校验已存在的最近祖先，再 create（避免拒写路径先落盘）
    ensure_output_dir(&dir)?;

    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .map(sanitize_stem)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "video".into());
    let label = action_label(mode, format);
    let ext = format.as_str();
    let base = format!("{stem}.{label}.{ext}");
    let mut candidate = dir.join(&base);
    if !candidate.exists() {
        return Ok(candidate);
    }
    for i in 1..1000 {
        candidate = dir.join(format!("{stem}.{label}-{i}.{ext}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err("无法生成唯一输出文件名".into())
}

pub async fn run_job(
    app: AppHandle,
    req: RunRequest,
    on_event: Channel<VideoEvent>,
) -> Result<(), String> {
    if BUSY
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("已有转码任务进行中".into());
    }

    let token = CancellationToken::new();
    *lock_or_recover(&CANCEL) = Some(token.clone());

    let result = run_job_inner(app, req, on_event, token.clone()).await;

    BUSY.store(false, Ordering::SeqCst);
    *lock_or_recover(&CANCEL) = None;
    result
}

async fn run_job_inner(
    app: AppHandle,
    req: RunRequest,
    on_event: Channel<VideoEvent>,
    token: CancellationToken,
) -> Result<(), String> {
    let input = PathBuf::from(&req.input_path);
    if !crate::platform::path_guard::validate(&input) {
        let msg = "输入路径不安全或不存在".to_string();
        emit_both(
            &app,
            &on_event,
            VideoEvent::Error {
                message: msg.clone(),
            },
        );
        set_last(JobSnapshot {
            busy: false,
            last_output: None,
            last_error: Some(msg),
            last_percent: 0.0,
        });
        return Ok(());
    }

    let out_dir = req
        .output_dir
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(Path::new);
    let output = build_output_path(&input, out_dir, req.params.mode, req.params.format)?;
    let output_str = output.to_string_lossy().to_string();

    let bins = core::ensure_bins(&app).await?;
    let duration = if req.duration_secs > 0.0 {
        req.duration_secs
    } else {
        core::probe(&app, &req.input_path)
            .map(|m| m.duration_secs)
            .unwrap_or(0.0)
    };

    let started = VideoEvent::Started {
        output_path: output_str.clone(),
    };
    emit_both(&app, &on_event, started);
    set_last(JobSnapshot {
        busy: true,
        last_output: Some(output_str.clone()),
        last_error: None,
        last_percent: 0.0,
    });

    // 硬件路径失败一律软重试一次（VT 常见通用错误不含 videotoolbox 子串）；
    // 取消 / 超时不重试。encode_once 在 use_hw 时统一前缀 "hw encode failed"。
    let encode_result = match encode_once(
        &app,
        &bins,
        &req.input_path,
        &output_str,
        &req.params,
        false,
        duration,
        &on_event,
        &token,
    )
    .await
    {
        Ok(()) => Ok(()),
        Err(e) if e.starts_with("hw encode failed") && !token.is_cancelled() => {
            log::warn!("[video] hardware encode failed, retry software: {e}");
            let _ = std::fs::remove_file(&output);
            encode_once(
                &app,
                &bins,
                &req.input_path,
                &output_str,
                &req.params,
                true,
                duration,
                &on_event,
                &token,
            )
            .await
        }
        Err(e) => Err(e),
    };

    if token.is_cancelled() {
        let _ = std::fs::remove_file(&output);
        let msg = "已取消".to_string();
        emit_both(
            &app,
            &on_event,
            VideoEvent::Error {
                message: msg.clone(),
            },
        );
        set_last(JobSnapshot {
            busy: false,
            last_output: None,
            last_error: Some(msg),
            last_percent: 0.0,
        });
        return Ok(());
    }

    if let Err(e) = encode_result {
        let _ = std::fs::remove_file(&output);
        emit_both(&app, &on_event, VideoEvent::Error { message: e.clone() });
        set_last(JobSnapshot {
            busy: false,
            last_output: None,
            last_error: Some(e),
            last_percent: 0.0,
        });
        return Ok(());
    }

    let size_bytes = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
    if size_bytes == 0 {
        let _ = std::fs::remove_file(&output);
        let msg = "输出文件为空".to_string();
        emit_both(
            &app,
            &on_event,
            VideoEvent::Error {
                message: msg.clone(),
            },
        );
        set_last(JobSnapshot {
            busy: false,
            last_output: None,
            last_error: Some(msg),
            last_percent: 0.0,
        });
        return Ok(());
    }

    let done = VideoEvent::Done {
        output_path: output_str.clone(),
        size_bytes,
    };
    emit_both(&app, &on_event, done);
    set_last(JobSnapshot {
        busy: false,
        last_output: Some(output_str),
        last_error: None,
        last_percent: 100.0,
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn encode_once(
    app: &AppHandle,
    bins: &FfmpegBins,
    input: &str,
    output: &str,
    params: &EncodeParams,
    force_software: bool,
    duration_secs: f64,
    on_event: &Channel<VideoEvent>,
    token: &CancellationToken,
) -> Result<(), String> {
    let (args, use_hw) = build_ffmpeg_args(input, output, params, force_software);
    let mut child = TokioCommand::new(&bins.ffmpeg)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("启动 ffmpeg 失败: {e}"))?;

    let stdout = child.stdout.take().ok_or("无法获取 ffmpeg stdout")?;
    let stderr = child.stderr.take().ok_or("无法获取 ffmpeg stderr")?;

    let mut stdout_reader = BufReader::new(stdout).lines();
    let mut stderr_reader = BufReader::new(stderr).lines();
    let mut stderr_buf = String::new();
    let mut last_percent = 0.0f64;
    let mut speed = String::new();
    let mut out_time_us: u64 = 0;
    let mut stderr_done = false;

    // 整段 encode（进度循环 + wait）统一超时；sleep 用 pin 避免每轮 select 重置
    let deadline = tokio::time::sleep(JOB_TIMEOUT);
    tokio::pin!(deadline);

    loop {
        tokio::select! {
            biased;
            _ = token.cancelled() => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Err("已取消".to_string());
            }
            _ = &mut deadline => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Err("转码超时".to_string());
            }
            line = stdout_reader.next_line() => {
                match line {
                    Ok(Some(line)) => {
                        if let Some((k, v)) = line.split_once('=') {
                            match k.trim() {
                                // ffmpeg progress：out_time_ms 名虽带 ms，单位实为微秒
                                "out_time_ms" | "out_time_us" => {
                                    if let Ok(us) = v.trim().parse::<u64>() {
                                        out_time_us = us;
                                    }
                                }
                                "speed" => {
                                    speed = v.trim().to_string();
                                }
                                _ => {}
                            }
                            if duration_secs > 0.0 {
                                let t = out_time_us as f64 / 1_000_000.0;
                                let percent =
                                    ((t / duration_secs) * 100.0).clamp(0.0, 99.5);
                                if (percent - last_percent).abs() >= 0.5 {
                                    last_percent = percent;
                                    emit_both(
                                        app,
                                        on_event,
                                        VideoEvent::Progress {
                                            percent,
                                            time_secs: t,
                                            speed: speed.clone(),
                                        },
                                    );
                                    set_last(JobSnapshot {
                                        busy: true,
                                        last_output: Some(output.to_string()),
                                        last_error: None,
                                        last_percent: percent,
                                    });
                                }
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(e) => return Err(format!("读取进度失败: {e}")),
                }
            }
            line = stderr_reader.next_line(), if !stderr_done => {
                match line {
                    Ok(Some(line)) => {
                        if stderr_buf.len() < 8192 {
                            stderr_buf.push_str(&line);
                            stderr_buf.push('\n');
                        }
                    }
                    Ok(None) | Err(_) => {
                        stderr_done = true;
                    }
                }
            }
        }
    }

    if !stderr_done {
        loop {
            tokio::select! {
                biased;
                _ = token.cancelled() => {
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    return Err("已取消".to_string());
                }
                _ = &mut deadline => {
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    return Err("转码超时".to_string());
                }
                line = stderr_reader.next_line() => {
                    match line {
                        Ok(Some(line)) => {
                            if stderr_buf.len() < 8192 {
                                stderr_buf.push_str(&line);
                                stderr_buf.push('\n');
                            }
                        }
                        Ok(None) | Err(_) => break,
                    }
                }
            }
        }
    }

    let status = tokio::select! {
        biased;
        _ = token.cancelled() => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err("已取消".to_string());
        }
        _ = &mut deadline => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err("转码超时".to_string());
        }
        status = child.wait() => {
            status.map_err(|e| format!("等待 ffmpeg 退出失败: {e}"))?
        }
    };

    if !status.success() {
        // use_hw 本趟失败一律标记，外层统一软重试（VT 错误信息不稳定，子串匹配易漏）
        if use_hw {
            return Err(format!("hw encode failed: {stderr_buf}"));
        }
        return Err(if stderr_buf.is_empty() {
            format!("ffmpeg 退出码 {:?}", status.code())
        } else {
            let tail: String = stderr_buf
                .lines()
                .rev()
                .take(6)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n");
            format!("ffmpeg 失败:\n{tail}")
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extensions::video::args::{OutputFormat, VideoMode};

    #[test]
    fn build_output_path_unique() {
        let dir = std::env::temp_dir().join(format!("voidnix-video-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let input = dir.join("clip.mp4");
        std::fs::write(&input, b"x").unwrap();
        // path_guard needs canonicalize-able path — file exists
        let out = build_output_path(&input, Some(&dir), VideoMode::Compress, OutputFormat::Mp4)
            .expect("output path");
        assert!(out
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains("compressed"));
        assert_eq!(out.extension().and_then(|e| e.to_str()), Some("mp4"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
