use crate::runtime::storage::ext_data_dir;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::LazyLock;
use tauri::{AppHandle, Emitter};

/// mihomo（Clash.Meta）核心版本与 darwin asset sha256（.gz，来自 release digest）。
/// 运行时按需下载，不编译期嵌入（避免二进制膨胀 + 可热更新）。
/// 常量仅作 GitHub API 不可达时的 fallback；正常路径走 fetch_latest_asset 拿最新版本。
const MIHOMO_VERSION: &str = "v1.19.27";
const SHA256_ARM64: &str = "3617c9d8a5a55aecfe1ebd0f55ff59f2706c8ad68fd65c6c4e5f7cf2b74263f1";
const SHA256_AMD64: &str = "5392bea435a1c4b0a496571daafa977f744207cfafac18fb78a9b7d0747585c2";
const RELEASE_BASE: &str = "https://github.com/MetaCubeX/mihomo/releases/download";
/// 国内镜像前缀（GitHub 直连慢/不稳），镜像仅代理转发，sha256 校验保证内容一致。
pub(crate) const MIRROR_PREFIX: &str = "https://gh-proxy.com/";
/// Geo 数据库 release 基址（geoip.metadb / geosite.dat / geoip.dat）。
pub(crate) const GEO_RELEASE_BASE: &str =
    "https://github.com/MetaCubeX/meta-rules-dat/releases/download/latest";
/// GitHub API latest release 端点（不经镜像：gh-proxy 仅代理 release 下载，不转发 API）。
const LATEST_API: &str = "https://api.github.com/repos/MetaCubeX/mihomo/releases/latest";

/// 核心下载进行中标记（全局，ensure_bin 期间置 true，供 status 查询）。
static DOWNLOADING: AtomicBool = AtomicBool::new(false);

/// 下载串行化锁（配合 ensure_bin double-check，防并发下载损坏 .gz）。
static DOWNLOAD_LOCK: LazyLock<tokio::sync::Mutex<()>> =
    LazyLock::new(|| tokio::sync::Mutex::new(()));

/// mihomo -v 版本号提取正则（LazyLock 避免每次 core_version 回退重编译）。
static VERSION_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"v\d+\.\d+\.\d+").expect("invalid version regex"));

/// 核心状态（供前端列表「核心」项展示版本号/下载状态）。
#[derive(Serialize)]
pub struct CoreStatus {
    pub downloaded: bool,
    pub version: String,
    pub downloading: bool,
}

/// mihomo release asset：版本号 + 下载 URL（已拼镜像）+ sha256（纯 hex）。
/// 由 fetch_latest_asset 从 GitHub API 解析，或 fallback_asset 用常量拼装。
pub(crate) struct CoreAsset {
    pub version: String,
    pub url: String,
    pub sha256: String,
}

/// 更新检查结果：current 为空（未下载/版本未知）时强制 has_update=false。
#[derive(Serialize)]
pub struct UpdateInfo {
    pub has_update: bool,
    pub current: String,
    pub latest: String,
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
/// 升级 mihomo：用户手动删除 mihomo 文件触发重下（或后续加「更新核心」入口）。
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

/// 查询核心状态：下载中 / 已下载（含版本号）/ 未下载。
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

/// mihomo asset 架构名（arm64 / amd64）。
fn mihomo_arch_slug() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "amd64"
    }
}

/// 从 GitHub API 拉取 latest release，匹配当前架构 asset 元数据。
/// 一次调用同时拿到 version（tag_name）+ download URL + sha256（asset.digest）。
/// 失败由调用方决定是否回退常量。gh-proxy 不转发 API，此处直连 api.github.com。
pub(crate) async fn fetch_latest_asset() -> Result<CoreAsset, String> {
    let arch = mihomo_arch_slug();
    let resp = crate::http::client()
        .get(LATEST_API)
        .header("User-Agent", "Voidnix")
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("拉取最新版本失败: {e}"))?
        .error_for_status()
        .map_err(|e| format!("拉取最新版本响应错误: {e}"))?;
    let v: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("解析 release JSON 失败: {e}"))?;
    let version = v
        .get("tag_name")
        .and_then(|t| t.as_str())
        .ok_or("release 缺 tag_name")?
        .to_string();
    // 精确串等匹配，排除 go120/go122/go124 等变体 asset
    let expected = format!("mihomo-darwin-{arch}-{version}.gz");
    let asset = v
        .get("assets")
        .and_then(|a| a.as_array())
        .and_then(|arr| {
            arr.iter()
                .find(|a| a.get("name").and_then(|n| n.as_str()) == Some(expected.as_str()))
        })
        .ok_or_else(|| format!("release 未找到 {expected}"))?;
    let dl = asset
        .get("browser_download_url")
        .and_then(|u| u.as_str())
        .ok_or("asset 缺 browser_download_url")?;
    let digest = asset
        .get("digest")
        .and_then(|d| d.as_str())
        .ok_or("asset 缺 digest")?;
    let sha256 = digest.strip_prefix("sha256:").unwrap_or(digest).to_string();
    Ok(CoreAsset {
        version,
        url: format!("{MIRROR_PREFIX}{dl}"),
        sha256,
    })
}

/// GitHub API 不可达时的 fallback：用常量版本 + sha256 拼 URL（与原硬编码逻辑等价）。
fn fallback_asset() -> CoreAsset {
    let arch = mihomo_arch_slug();
    let sha = if arch == "arm64" {
        SHA256_ARM64
    } else {
        SHA256_AMD64
    };
    let url = format!(
        "{MIRROR_PREFIX}{RELEASE_BASE}/{MIHOMO_VERSION}/mihomo-darwin-{arch}-{MIHOMO_VERSION}.gz"
    );
    CoreAsset {
        version: MIHOMO_VERSION.to_string(),
        url,
        sha256: sha.to_string(),
    }
}

/// 检查更新：拉 latest 版本号 → 比对本地版本。API 不可达时静默降级（has_update=false）。
pub async fn check_update(app: &AppHandle) -> UpdateInfo {
    let latest = match fetch_latest_asset().await {
        Ok(a) => a.version,
        Err(e) => {
            eprintln!("[proxy] 检查更新失败: {e}");
            return UpdateInfo {
                has_update: false,
                current: String::new(),
                latest: String::new(),
            };
        }
    };
    let current = core_status(app).version;
    UpdateInfo {
        has_update: !current.is_empty() && current != latest,
        current,
        latest,
    }
}

/// 下载 mihomo：经 runtime::binary_fetch 流式拉取 .gz → sha256 → gunzip → chmod，
/// 成功后写 version 缓存 + emit ready。进度事件 payload = FetchProgress（字段与旧 CoreProgress 一致）。
async fn download_core_async(app: &AppHandle) -> Result<(), String> {
    let asset = fetch_latest_asset().await.unwrap_or_else(|e| {
        eprintln!("[proxy] 拉取最新版本失败，回退常量版本: {e}");
        fallback_asset()
    });
    let dir = ext_data_dir(app, "proxy")?;
    let gz = dir.join("mihomo.gz");
    let bin = dir.join("mihomo");

    DOWNLOADING.store(true, Ordering::Relaxed);
    let result = crate::runtime::binary_fetch::fetch(
        app,
        crate::runtime::binary_fetch::BinaryFetch {
            urls: vec![asset.url],
            gz_path: &gz,
            bin_path: &bin,
            expected_sha256: &asset.sha256,
            progress_event: "proxy-core-progress",
            progress_base: 0,
        },
    )
    .await;
    DOWNLOADING.store(false, Ordering::Relaxed);
    result?;

    // version 缓存 + ready 信号（binary_fetch 只管下载/校验/解压/chmod）
    std::fs::write(bin.parent().unwrap().join("mihomo.version"), &asset.version)
        .map_err(|e| e.to_string())?;
    let _ = app.emit("proxy-core-ready", ());
    Ok(())
}

/// 删除 mihomo binary + version 缓存。update 流程前调用，确保下次 ensure_bin 重下最新。
/// 文件不存在视为成功（幂等）。
pub(crate) fn remove_core_files(app: &AppHandle) -> Result<(), String> {
    let dir = ext_data_dir(app, "proxy")?;
    let _ = std::fs::remove_file(dir.join("mihomo"));
    let _ = std::fs::remove_file(dir.join("mihomo.version"));
    Ok(())
}
