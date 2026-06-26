use crate::runtime::storage::ext_data_dir;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::AppHandle;

/// mihomo（Clash.Meta）核心版本与 darwin asset sha256（.gz，来自 release digest）。
/// 运行时按需下载，不编译期嵌入（避免二进制膨胀 + 可热更新）。
const MIHOMO_VERSION: &str = "v1.19.27";
const SHA256_ARM64: &str = "3617c9d8a5a55aecfe1ebd0f55ff59f2706c8ad68fd65c6c4e5f7cf2b74263f1";
const SHA256_AMD64: &str = "5392bea435a1c4b0a496571daafa977f744207cfafac18fb78a9b7d0747585c2";
const RELEASE_BASE: &str = "https://github.com/MetaCubeX/mihomo/releases/download";
/// 国内镜像前缀（GitHub 直连慢/不稳），镜像仅代理转发，sha256 校验保证内容一致。
const MIRROR_PREFIX: &str = "https://gh-proxy.com/";

/// 内核下载进行中标记（全局，ensure_bin 期间置 true，供 status 查询）。
static DOWNLOADING: AtomicBool = AtomicBool::new(false);

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
    /// TUN 模式：true 时 config.yaml 含 tun+dns 段，且 mihomo 须以 root 启动（osascript 提权）。
    pub tun: bool,
}

/// 托管 mihomo 子进程：Drop 自动 kill+wait，覆盖 app 正常退出场景。
/// panic=abort 下 Drop 不跑，mihomo 可能残留——首期接受，后续可加启动期 cleanup 兜底。
pub(crate) struct ManagedChild(Option<Child>);

impl ManagedChild {
    pub(crate) fn shutdown(&mut self) {
        if let Some(mut child) = self.0.take() {
            drop(child.stdin.take());
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for ManagedChild {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// 代理核心运行状态：持有可能在跑的 mihomo 子进程。
pub struct ProxyCore {
    pub process: Mutex<Option<ManagedChild>>,
}

impl ProxyCore {
    pub fn new() -> Self {
        Self {
            process: Mutex::new(None),
        }
    }
}

/// mihomo binary 写盘路径（app_data_dir/extensions/proxy/mihomo）。
fn bin_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(ext_data_dir(app, "proxy")?.join("mihomo"))
}

/// mihomo 运行配置路径（app_data_dir/extensions/proxy/config.yaml）。
pub(crate) fn run_config_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(ext_data_dir(app, "proxy")?.join("config.yaml"))
}

/// 确保 mihomo binary 就绪：已存在则直接复用，否则下载（spawn_blocking 避免阻塞 executor）。
///
/// 不做版本强校验——避免「binary 已在但因 version 文件缺失/不匹配而强制重下」导致的网络卡死。
/// 升级 mihomo：用户手动删除 mihomo 文件触发重下（或后续加「更新内核」入口）。
pub async fn ensure_bin(app: &AppHandle) -> Result<PathBuf, String> {
    let bin = bin_path(app)?;
    if bin.exists() {
        return Ok(bin);
    }
    download_core_async(app).await?;
    Ok(bin)
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
    let re = regex::Regex::new(r"v\d+\.\d+\.\d+").ok()?;
    let m = re.find(&s)?;
    let v = m.as_str().to_string();
    let _ = std::fs::write(&vf, &v); // 缓存，避免重复跑 mihomo -v
    Some(v)
}

/// 下载 mihomo：reqwest 流式拉取 .gz（推送进度事件）→ sha256 校验 → gunzip 解压 → chmod。
/// 进度通过 app.emit("proxy-core-progress", u8) 推送；gunzip 依赖系统命令（macOS 自带）。
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

    // 流式下载 + 进度推送
    let resp = crate::http::client()
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
        if let Some(t) = total {
            let pct = (received * 100 / t).min(100) as u8;
            let _ = app.emit("proxy-core-progress", pct);
        }
    }
    drop(file);

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
    let _ = app.emit("proxy-core-progress", 100u8);
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

/// 写入 binary（确保就绪）+ 合并订阅生成 config.yaml，返回 (bin_path, run_dir)。
/// user 模式 spawn 与 root 模式 spawn_root 共用。
pub(crate) async fn prepare(
    app: &AppHandle,
    params: &RunParams,
) -> Result<(PathBuf, PathBuf), String> {
    let bin = ensure_bin(app).await?;
    let yaml = super::subscription::build_run_config(app, params)?;
    std::fs::write(run_config_path(app)?, yaml).map_err(|e| e.to_string())?;
    let dir = ext_data_dir(app, "proxy")?;
    Ok((bin, dir))
}

/// 启动 mihomo 子进程（user 模式）：返回 ManagedChild 由调用方存入 State。
///
/// -d 指定运行目录（config.yaml 所在），stderr 继承以便 dev 调试查看 mihomo 日志。
pub async fn spawn(app: &AppHandle, params: &RunParams) -> Result<ManagedChild, String> {
    let (bin, dir) = prepare(app, params).await?;
    let child = Command::new(&bin)
        .arg("-d")
        .arg(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("failed to spawn mihomo: {e}"))?;
    Ok(ManagedChild(Some(child)))
}
