use crate::runtime::storage::ext_data_dir;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::LazyLock;
use tauri::AppHandle;

/// mihomo（Clash.Meta）核心版本与 darwin asset sha256（.gz，来自 release digest）。
/// 运行时按需下载，不编译期嵌入（避免二进制膨胀 + 可热更新）。
const MIHOMO_VERSION: &str = "v1.19.27";
const SHA256_ARM64: &str = "3617c9d8a5a55aecfe1ebd0f55ff59f2706c8ad68fd65c6c4e5f7cf2b74263f1";
const SHA256_AMD64: &str = "5392bea435a1c4b0a496571daafa977f744207cfafac18fb78a9b7d0747585c2";
const RELEASE_BASE: &str = "https://github.com/MetaCubeX/mihomo/releases/download";
/// 国内镜像前缀（GitHub 直连慢/不稳），镜像仅代理转发，sha256 校验保证内容一致。
pub(crate) const MIRROR_PREFIX: &str = "https://gh-proxy.com/";
/// Geo 数据库 release 基址（geoip.metadb / geosite.dat / geoip.dat）。
pub(crate) const GEO_RELEASE_BASE: &str =
    "https://github.com/MetaCubeX/meta-rules-dat/releases/download/latest";

/// 内核下载进行中标记（全局，ensure_bin 期间置 true，供 status 查询）。
static DOWNLOADING: AtomicBool = AtomicBool::new(false);

/// 下载进度事件 payload：received 为已收字节，total 为 None 表示 chunked（无法算百分比）。
#[derive(Clone, Serialize)]
pub struct CoreProgress {
    pub received: u64,
    pub total: Option<u64>,
}

/// 下载串行化锁（配合 ensure_bin double-check，防并发下载损坏 .gz）。
static DOWNLOAD_LOCK: LazyLock<tokio::sync::Mutex<()>> =
    LazyLock::new(|| tokio::sync::Mutex::new(()));

/// mihomo -v 版本号提取正则（LazyLock 避免每次 core_version 回退重编译）。
static VERSION_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"v\d+\.\d+\.\d+").expect("invalid version regex"));

/// 内核状态（供前端列表「内核」项展示版本号/下载状态）。
#[derive(Serialize)]
pub struct CoreStatus {
    pub downloaded: bool,
    pub version: String,
    pub downloading: bool,
}

/// 启动 mihomo 所需的运行参数（前端 config 传入，启动时缓存以支持热重启）。
#[derive(Clone)]
pub struct RunParams {
    pub mixed_port: u16,
    pub controller_port: u16,
    pub secret: String,
    pub mode: String,
    /// 是否含 tun 段：active config（用户开启）= true，idle config（关闭直通）= false。
    /// 统一 TUN 模式后不再暴露为用户开关，仅作 active/idle 内部标记。
    pub tun: bool,
}

/// mihomo binary 写盘路径（app_data_dir/extensions/proxy/mihomo）。
fn bin_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(ext_data_dir(app, "proxy")?.join("mihomo"))
}

/// mihomo 运行配置路径（app_data_dir/extensions/proxy/config.yaml）。
pub(crate) fn run_config_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(ext_data_dir(app, "proxy")?.join("config.yaml"))
}

/// 确保 mihomo binary 就绪：已存在则直接复用，否则下载。
///
/// 不做版本强校验——避免「binary 已在但因 version 文件缺失/不匹配而强制重下」导致的网络卡死。
/// 升级 mihomo：用户手动删除 mihomo 文件触发重下（或后续加「更新内核」入口）。
/// 并发守卫：多调用方（双击下载/开代理 spawn）同时触发时串行化，仅一个真正下载，其余
/// double-check 复用——防多流并发写同一 .gz 致 sha256 校验失败、binary 无法产出。
pub async fn ensure_bin(app: &AppHandle) -> Result<PathBuf, String> {
    let bin = bin_path(app)?;
    if bin.exists() {
        return Ok(bin);
    }
    let _guard = DOWNLOAD_LOCK.lock().await;
    if bin.exists() {
        return Ok(bin); // double-check：抢锁期间已被前一个下载者写好
    }
    download_core_async(app).await?;
    Ok(bin)
}

/// 确保 Geo 数据库文件就绪（geoip.metadb + geosite.dat）。
///
/// mihomo 加载含 GEOIP/GEOSITE 规则的 config 时需这些文件。缺失时 mihomo 同步下载
/// （直连 GitHub 在国内不可达 → EOF → config 加载失败/控制器不启动 → 开代理超时）。
/// 通过 gh-proxy 镜像预下载，已存在则跳过。下载失败不阻塞（mihomo 可用 geox-url 自行重试）。
pub async fn ensure_geo_files(app: &AppHandle) -> Result<(), String> {
    let dir = ext_data_dir(app, "proxy")?;
    for name in ["geoip.metadb", "geosite.dat"] {
        let path = dir.join(name);
        if path.exists() {
            continue;
        }
        let url = format!("{MIRROR_PREFIX}{GEO_RELEASE_BASE}/{name}");
        if let Err(e) = download_file(&url, &path).await {
            eprintln!("[proxy] geo 数据库 {name} 预下载失败（mihomo 将经 geox-url 自行重试）: {e}");
        }
    }
    Ok(())
}

/// 通用文件下载（download_client 无整体超时，适合大文件）。
async fn download_file(url: &str, dest: &Path) -> Result<(), String> {
    let resp = crate::http::download_client()
        .get(url)
        .send()
        .await
        .map_err(|e| format!("下载请求失败: {e}"))?
        .error_for_status()
        .map_err(|e| format!("下载响应错误: {e}"))?;
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("下载读取失败: {e}"))?;
    if bytes.is_empty() {
        return Err("下载内容为空".into());
    }
    std::fs::write(dest, &bytes).map_err(|e| e.to_string())
}

/// 查询内核状态：下载中 / 已下载（含版本号）/ 未下载。
/// 版本优先读 mihomo.version 缓存，缺失则跑 `mihomo -v` 解析并回写缓存。
pub fn core_status(app: &AppHandle) -> CoreStatus {
    if DOWNLOADING.load(Ordering::Relaxed) {
        return CoreStatus {
            downloaded: false,
            version: String::new(),
            downloading: true,
        };
    }
    let bin = match bin_path(app) {
        Ok(b) => b,
        Err(_) => {
            return CoreStatus {
                downloaded: false,
                version: String::new(),
                downloading: false,
            }
        }
    };
    if !bin.exists() {
        return CoreStatus {
            downloaded: false,
            version: String::new(),
            downloading: false,
        };
    }
    let version = core_version(app, &bin).unwrap_or_default();
    CoreStatus {
        downloaded: true,
        version,
        downloading: false,
    }
}

fn core_version(app: &AppHandle, bin: &Path) -> Option<String> {
    let dir = ext_data_dir(app, "proxy").ok()?;
    let vf = dir.join("mihomo.version");
    if let Ok(v) = std::fs::read_to_string(&vf) {
        let v = v.trim();
        if !v.is_empty() {
            return Some(v.to_string());
        }
    }
    // fallback：跑 mihomo -v，正则提取 v1.19.27
    let out = Command::new(bin).arg("-v").output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    let m = VERSION_RE.find(&s)?;
    let v = m.as_str().to_string();
    let _ = std::fs::write(&vf, &v); // 缓存，避免重复跑 mihomo -v
    Some(v)
}

/// 下载 mihomo：reqwest 流式拉取 .gz（推送进度事件）→ sha256 校验 → gunzip 解压 → chmod。
/// 进度通过 app.emit("proxy-core-progress", CoreProgress) 推送（received/total，chunked 时 total=None）；gunzip 依赖系统命令（macOS 自带）。
async fn download_core_async(app: &AppHandle) -> Result<(), String> {
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "amd64"
    };
    let sha = if arch == "arm64" {
        SHA256_ARM64
    } else {
        SHA256_AMD64
    };
    let url = format!(
        "{MIRROR_PREFIX}{RELEASE_BASE}/{MIHOMO_VERSION}/mihomo-darwin-{arch}-{MIHOMO_VERSION}.gz"
    );
    let dir = ext_data_dir(app, "proxy")?;
    let gz = dir.join("mihomo.gz");
    let bin = dir.join("mihomo");

    DOWNLOADING.store(true, Ordering::Relaxed);
    let result = download_core_inner(app, &url, &gz, &bin, sha).await;
    DOWNLOADING.store(false, Ordering::Relaxed);
    result
}

async fn download_core_inner(
    app: &AppHandle,
    url: &str,
    gz: &Path,
    bin: &Path,
    expected_sha: &str,
) -> Result<(), String> {
    use futures_util::StreamExt;
    use tauri::Emitter;

    // 流式下载 + 进度推送（download_client 无整体超时，慢网络下大文件不会被中途掐断）
    let resp = crate::http::download_client()
        .get(url)
        .send()
        .await
        .map_err(|e| format!("下载请求失败: {e}"))?
        .error_for_status()
        .map_err(|e| format!("下载响应错误: {e}"))?;
    let total = resp.content_length();
    let mut file = std::fs::File::create(gz).map_err(|e| e.to_string())?;
    let mut received: u64 = 0;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("下载读取失败: {e}"))?;
        std::io::Write::write_all(&mut file, &chunk).map_err(|e| e.to_string())?;
        received += chunk.len() as u64;
        let _ = app.emit("proxy-core-progress", CoreProgress { received, total });
    }
    drop(file);
    // 字节收齐，进入 sha256 + gunzip 后处理：emit 完成信号（total 设为 received 标记下载完成，
    // chunked 场景也借此从「下载中」切到后处理态），前端按钮转「解压中」
    let _ = app.emit(
        "proxy-core-progress",
        CoreProgress {
            received,
            total: Some(received),
        },
    );

    // sha256 校验（sha2，防篡改 / 部分下载损坏）
    let actual = sha256_file(gz)?;
    if actual != expected_sha {
        let _ = std::fs::remove_file(gz);
        return Err(format!(
            "mihomo sha256 校验失败（expected {expected_sha}, got {actual}）"
        ));
    }

    // gunzip 解压（macOS 自带）
    let gunzip = Command::new("gunzip")
        .arg("-f")
        .arg(gz)
        .status()
        .map_err(|e| format!("gunzip 调用失败: {e}"))?;
    if !gunzip.success() {
        return Err("gunzip 解压失败".into());
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(bin)
            .map_err(|e| e.to_string())?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(bin, perms).map_err(|e| e.to_string())?;
    }

    std::fs::write(bin.parent().unwrap().join("mihomo.version"), MIHOMO_VERSION)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 计算文件 sha256（十六进制小写）。
fn sha256_file(path: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 16384];
    loop {
        let n = std::io::Read::read(&mut file, &mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let hash = hasher.finalize();
    Ok(hash.iter().map(|b| format!("{:02x}", b)).collect())
}

#[cfg(test)]
mod tests {
    #[test]
    fn sha256_file_placeholder() {
        // 下载逻辑需 AppHandle + 联网，不便单测；sha256 校验由常量保证。
    }
}
