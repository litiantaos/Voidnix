//! 外部 binary 按需下载流水线：流式拉取 .gz → sha256 校验 → gunzip → chmod。
//!
//! proxy（mihomo）与 video（ffmpeg/ffprobe）共用，消除两处下载实现的双轨。
//! 统一为**流式落盘**（proxy 范式），避免 video 旧实现把整包读入内存（ffmpeg gz
//! 可达 ~100MB 常驻 Vec）。镜像回退在内部依次尝试 spec.urls。

use std::path::Path;
use tauri::{AppHandle, Emitter};

/// 进度事件 payload（proxy/video 共用同名同构 CoreProgress）。
#[derive(serde::Serialize, Clone, Copy)]
pub struct FetchProgress {
    pub received: u64,
    pub total: Option<u64>,
}

/// 单个 binary 的下载规格。
pub struct BinaryFetch<'a> {
    /// 候选 URL（镜像在前，直连兜底；依次尝试至首个成功）
    pub urls: Vec<String>,
    /// .gz 临时落盘路径
    pub gz_path: &'a Path,
    /// 解压后 binary 目标路径
    pub bin_path: &'a Path,
    /// 期望 sha256（十六进制小写）
    pub expected_sha256: &'a str,
    /// 进度事件名（"" 则不推）
    pub progress_event: &'a str,
    /// 多文件场景的进度累计偏移（单文件传 0）
    pub progress_base: u64,
}

/// 下载并就绪一个 binary：流式落盘 → sha256 → gunzip → chmod 0o755。
///
/// URL 回退：任一 URL 走完全流程（下载 + 校验 + 解压）即成功；失败清残文件后试下一个。
/// 进度事件 payload = `FetchProgress { received, total }`，received 含 progress_base。
pub async fn fetch(app: &AppHandle, spec: BinaryFetch<'_>) -> Result<(), String> {
    if spec.urls.is_empty() {
        return Err("无可用的下载源".into());
    }

    let mut last_err = String::new();
    for (i, url) in spec.urls.iter().enumerate() {
        let _ = std::fs::remove_file(spec.gz_path);
        match download_one(
            app,
            url,
            spec.gz_path,
            spec.progress_event,
            spec.progress_base,
        )
        .await
        {
            Ok(()) => match finalize(spec.gz_path, spec.bin_path, spec.expected_sha256) {
                Ok(()) => return Ok(()),
                Err(e) => {
                    last_err = e;
                    let _ = std::fs::remove_file(spec.gz_path);
                    let _ = std::fs::remove_file(spec.bin_path);
                }
            },
            Err(e) => {
                last_err = e;
                let _ = std::fs::remove_file(spec.gz_path);
            }
        }
        if i == 0 && spec.urls.len() > 1 {
            log::warn!("[binary_fetch] 镜像下载失败，回退直连: {last_err}");
        }
    }
    Err(last_err)
}

/// 流式下载单个 URL 到 gz（逐 chunk write_all + 推进度）。下载完发完成信号。
async fn download_one(
    app: &AppHandle,
    url: &str,
    gz: &Path,
    progress_event: &str,
    progress_base: u64,
) -> Result<(), String> {
    use futures_util::StreamExt;

    let resp = crate::http::stream_client()
        .get(url)
        .send()
        .await
        .map_err(|e| format!("下载请求失败: {e}"))?
        .error_for_status()
        .map_err(|e| format!("下载响应错误: {e}"))?;
    let total = resp.content_length().map(|t| progress_base + t);

    let mut file = std::fs::File::create(gz).map_err(|e| e.to_string())?;
    let mut received: u64 = 0;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("下载读取失败: {e}"))?;
        std::io::Write::write_all(&mut file, &chunk).map_err(|e| e.to_string())?;
        received += chunk.len() as u64;
        if !progress_event.is_empty() {
            let _ = app.emit(
                progress_event,
                FetchProgress {
                    received: progress_base + received,
                    total,
                },
            );
        }
    }
    drop(file);
    // 字节收齐：发完成信号（total = received），让前端切到后处理态（解压中）
    if !progress_event.is_empty() {
        let _ = app.emit(
            progress_event,
            FetchProgress {
                received: progress_base + received,
                total: Some(progress_base + received),
            },
        );
    }
    Ok(())
}

/// sha256 校验 → gunzip 解压 → rename（若需）→ chmod 0o755。
fn finalize(gz: &Path, bin: &Path, expected_sha: &str) -> Result<(), String> {
    let actual = crate::runtime::storage::sha256_file(gz)?;
    if actual != expected_sha {
        let _ = std::fs::remove_file(gz);
        return Err(format!(
            "sha256 校验失败（expected {expected_sha}, got {actual}）"
        ));
    }

    // gunzip -f：解压并删除 .gz，输出去 .gz 后缀的同名文件
    let gunzip = std::process::Command::new("gunzip")
        .arg("-f")
        .arg(gz)
        .status()
        .map_err(|e| format!("gunzip 调用失败: {e}"))?;
    if !gunzip.success() {
        let _ = std::fs::remove_file(bin);
        return Err("gunzip 解压失败".into());
    }

    // gunzip 输出名 = 去掉 .gz；若与目标 bin 不一致则 rename
    let gunzipped = gz.with_extension("");
    if gunzipped != bin && gunzipped.exists() {
        std::fs::rename(&gunzipped, bin).map_err(|e| e.to_string())?;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(bin)
            .map_err(|e| e.to_string())?
            .permissions();
        perms.set_mode(0o755);
        if let Err(e) = std::fs::set_permissions(bin, perms) {
            let _ = std::fs::remove_file(bin);
            return Err(e.to_string());
        }
    }
    Ok(())
}
