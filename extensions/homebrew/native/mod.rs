//! Homebrew 管理扩展：包列表 + 服务管理 + 包详情 + 一键更新 + 卸载。
//!
//! 命令：
//! - `brew_status`：Homebrew 版本 + 全部已安装包（版本 / 最新版 / 描述 / 是否可升级）
//! - `brew_services`：列出 brew services 状态
//! - `brew_info`：单个包详情（依赖 / 反向依赖 / 版本 / 描述）
//! - `brew_run_state`：查询当前 brew_run 运行态（operation + step），前端组件销毁后仍可查询
//! - `brew_run`：流式执行 brew 子命令（update→upgrade→cleanup→autoremove / uninstall→autoremove / services start|stop|restart）
//!
//! brew 可执行路径硬探测（/opt/homebrew/bin/brew Apple Silicon / /usr/local/bin/brew Intel），
//! 不依赖 GUI 进程 PATH（可能不含 brew bin 目录）。
//!
//! **性能**：所有 brew 命令经 `brew_command` 统一注入 `HOMEBREW_NO_AUTO_UPDATE=1`（禁止隐式
//! 自动更新，消除联网拉取元数据的等待——实测首次加载从 ~40s 降至 ~1s）。
//! `brew_status` 用 `brew info --json=v2 --installed` 一次取全部已安装包（名称/描述/已安装版本），
//! 并发 `brew outdated --json=v2` 做过期检测，共 3 次 brew 进程（原先 6 次）。
//!
//! **后台元数据刷新**：`HOMEBREW_NO_AUTO_UPDATE=1` 的代价是 `brew outdated` 只对比本地 api 缓存，
//! 元数据陈旧时进入视图永远查不到新版本。`brew_status` 检测 api 元数据 mtime 超 24h（brew 自身
//! auto-update 同款节律与判定源）时后台 spawn `brew update`（复用 RunGuard 占用 BREW_RUNNING，
//! 完成时 Drop 发 `brew-run-done` 驱动前端重拉），快路径立即返回缓存态 + `refreshing` 标志；
//! 失败重试受 10min 冷却约束（完成事件驱动的重拉若 mtime 未变会无限再 spawn）。
//!
//! **运行态持久化**：`BREW_RUNNING`（LazyLock<Mutex<Option<BrewRunState>>>）跨组件生命周期持久化，
//! `brew_run` 经 RAII guard（`RunGuard`）占位 + 逐步更新 step，Drop 时自动清空 + emit `brew-run-done`
//! 事件。前端组件因窗口隐藏被 KeepAlive 卸载后，重开时经 `brew_run_state` 查询残留态，
//! 防止重复触发 + 恢复进度显示。guard 拒绝并发 `brew_run` 调用。

use crate::runtime::registry::Extension;
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime};
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

pub struct HomebrewExtension;

#[async_trait::async_trait]
impl Extension for HomebrewExtension {
    fn id(&self) -> &'static str {
        "homebrew"
    }
}

// ============================================================================
// 类型
// ============================================================================

#[derive(serde::Serialize)]
pub struct BrewEvent {
    /// "line" | "step" | "done" | "error"
    kind: &'static str,
    text: String,
}

#[derive(serde::Serialize)]
pub struct InstalledPackage {
    pub name: String,
    /// "formula" | "cask"
    pub kind: String,
    pub desc: String,
    /// 当前已安装版本
    pub version: String,
    /// 最新可用版本（空 = 已是最新）
    pub new_version: String,
}

/// `brew info --json=v2 --installed` 解析中间结构。
struct RawPackage {
    name: String,
    kind: String,
    desc: String,
    version: String,
}

#[derive(serde::Serialize)]
pub struct BrewStatus {
    pub version: String,
    pub packages: Vec<InstalledPackage>,
    pub has_update: bool,
    /// 元数据陈旧，后台 `brew update` 已发起或进行中（完成经 `brew-run-done` 驱动前端重拉）
    pub refreshing: bool,
}

#[derive(serde::Serialize)]
pub struct BrewService {
    pub name: String,
    pub status: String,
}

#[derive(serde::Serialize, Clone)]
pub struct PackageSummary {
    pub name: String,
    pub version: String,
    pub desc: String,
}

#[derive(serde::Serialize)]
pub struct BrewInfo {
    pub desc: String,
    pub deps: Vec<PackageSummary>,
    pub uses: Vec<PackageSummary>,
}

/// 当前 brew_run 运行态（跨组件生命周期持久化，窗口隐藏/重开后仍可查询）。
#[derive(serde::Serialize, Clone)]
pub struct BrewRunState {
    /// "update_upgrade" / "uninstall" / "services_start" 等
    pub operation: String,
    /// 当前正在执行的步骤名（如 "update"、"upgrade"、"uninstall"）
    pub step: String,
}

/// 全局运行态：Some = 有操作进行中，None = 空闲。
/// 前端组件销毁后仍可经 brew_run_state 命令查询。
static BREW_RUNNING: LazyLock<Mutex<Option<BrewRunState>>> = LazyLock::new(|| Mutex::new(None));

/// 尝试占用运行态。已有操作进行中则返回 false（防并发）。
fn try_set_running(operation: &str) -> bool {
    let mut guard = BREW_RUNNING.lock().unwrap();
    if guard.is_some() {
        return false;
    }
    *guard = Some(BrewRunState {
        operation: operation.to_string(),
        step: String::new(),
    });
    true
}

/// 清空运行态，返回之前的值（guard Drop 时调用）。
fn take_running() -> Option<BrewRunState> {
    BREW_RUNNING.lock().unwrap().take()
}

/// RAII guard：创建时占位 + 设 operation，Drop 时清空 + emit brew-run-done。
/// 确保任何退出路径（正常返回 / ?传播错误 / panic unwind）都清理状态并通知前端。
struct RunGuard {
    app: AppHandle,
}

impl RunGuard {
    /// 尝试占用运行态。已有操作进行中则返回 None（调用方应拒绝）。
    fn try_acquire(app: AppHandle, operation: &str) -> Option<Self> {
        if try_set_running(operation) {
            Some(RunGuard { app })
        } else {
            None
        }
    }

    /// 更新当前步骤名（run_brew_step 前调用）。
    fn set_step(&self, step: &str) {
        if let Some(state) = BREW_RUNNING.lock().unwrap().as_mut() {
            state.step = step.to_string();
        }
    }
}

impl Drop for RunGuard {
    fn drop(&mut self) {
        let prev = take_running();
        let _ = self.app.emit("brew-run-done", prev);
    }
}

// ============================================================================
// 辅助
// ============================================================================

/// 探测 brew 可执行路径（Apple Silicon → Intel 顺序）。
fn brew_path() -> Option<&'static str> {
    ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"]
        .into_iter()
        .find(|p| std::path::Path::new(p).exists())
}

/// 确保 PATH 含系统 + Homebrew bin 目录（GUI 进程 PATH 可能不全）。
fn ensure_brew_path() -> String {
    let mut path = std::env::var("PATH").unwrap_or_default();
    for dir in [
        "/opt/homebrew/bin",
        "/opt/homebrew/sbin",
        "/usr/local/bin",
        "/usr/local/sbin",
        "/usr/bin",
        "/bin",
        "/usr/sbin",
        "/sbin",
    ] {
        if !path.contains(dir) {
            path = format!("{dir}:{path}");
        }
    }
    path
}

/// 构建 brew 命令：统一注入 PATH + `HOMEBREW_NO_AUTO_UPDATE=1`（禁止隐式自动更新，
/// 消除联网拉取元数据的等待）。
fn brew_command(brew: &str, path: &str) -> Command {
    let mut cmd = Command::new(brew);
    cmd.env("PATH", path).env("HOMEBREW_NO_AUTO_UPDATE", "1");
    cmd
}

// ============================================================================
// 后台元数据刷新
// ============================================================================

/// api 元数据最大年龄（对齐 brew auto-update 默认 24h 节律）
const METADATA_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
/// 后台 `brew update` 超时上限（网络挂死防 BREW_RUNNING 永久占用）
const METADATA_REFRESH_TIMEOUT: Duration = Duration::from_secs(120);
/// 失败重试冷却：完成事件驱动前端重拉会立刻再调 `brew_status`，mtime 未变（上次 update 失败）时
/// 无冷却将形成 spawn → 失败 → brew-run-done → 重拉 → 再 spawn 的无限环。
/// 成功路径不受影响（mtime 已被触碰刷新，实测 `Already up-to-date` 也触碰）。
const REFRESH_RETRY_COOLDOWN: Duration = Duration::from_secs(10 * 60);
/// 上次刷新发起时刻（进程内）
static LAST_REFRESH_ATTEMPT: LazyLock<Mutex<Option<Instant>>> = LazyLock::new(|| Mutex::new(None));

/// 是否处于失败重试冷却窗口内
fn refresh_in_cooldown() -> bool {
    LAST_REFRESH_ATTEMPT
        .lock()
        .unwrap()
        .map(|t| t.elapsed() < REFRESH_RETRY_COOLDOWN)
        .unwrap_or(false)
}

/// 记账一次刷新发起（真正占位成功后调用）
fn note_refresh_attempt() {
    *LAST_REFRESH_ATTEMPT.lock().unwrap() = Some(Instant::now());
}

/// api 元数据 mtime → 是否陈旧（None = 从未 update，视为陈旧；时钟异常 elapsed Err 也强制刷新）。
fn metadata_stale_since(mtime: Option<SystemTime>) -> bool {
    mtime
        .map(|t| {
            t.elapsed()
                .map(|age| age > METADATA_MAX_AGE)
                .unwrap_or(true)
        })
        .unwrap_or(true)
}

/// 探测 api 元数据最新 mtime：brew 6 起 `internal/packages.<arch>.jws.json`（arch 随系统后缀变化），
/// brew 4/5 为根目录 `formula.jws.json`，取所有候选中的最新值。
fn api_metadata_mtime(api_dir: &Path) -> Option<SystemTime> {
    let mut newest = std::fs::metadata(api_dir.join("formula.jws.json"))
        .and_then(|m| m.modified())
        .ok();
    if let Ok(entries) = std::fs::read_dir(api_dir.join("internal")) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("packages.") && name.ends_with(".jws.json") {
                if let Ok(t) = entry.metadata().and_then(|m| m.modified()) {
                    newest = Some(newest.map_or(t, |n: SystemTime| n.max(t)));
                }
            }
        }
    }
    newest
}

/// 元数据陈旧则后台 spawn `brew update`。返回 true = 有刷新在途（本次发起或已在进行）。
/// 复用 RunGuard：占 BREW_RUNNING（brew_run_state 可查、brew_run 拒并发），
/// Drop 时 emit `brew-run-done` → 前端统一重拉拿到新元数据，无需新增事件。
fn spawn_metadata_refresh(app: &AppHandle, brew: &'static str, path: &str) -> bool {
    let home = std::env::var("HOME").unwrap_or_default();
    if home.is_empty() {
        return false;
    }
    let api_dir = Path::new(&home).join("Library/Caches/Homebrew/api");
    if !metadata_stale_since(api_metadata_mtime(&api_dir)) {
        return false;
    }
    // 冷却判定（记账在成功占位之后：并发竞争失败不消耗冷却窗口）
    if refresh_in_cooldown() {
        return false;
    }
    let Some(guard) = RunGuard::try_acquire(app.clone(), "update") else {
        return true; // 已有 brew 操作（含刷新）进行中
    };
    note_refresh_attempt();
    let path = path.to_string();
    tokio::spawn(async move {
        guard.set_step("update");
        let _ = tokio::time::timeout(
            METADATA_REFRESH_TIMEOUT,
            brew_command(brew, &path)
                .arg("update")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .output(),
        )
        .await;
        // guard Drop → BREW_RUNNING 清空 + emit brew-run-done → 前端重拉
    });
    true
}

/// 解析 brew outdated --json=v2 输出 → name → (installed, current) 映射。
/// 第三方 tap formula 的 name 是全限定名（如 `user/tap/pkg`），归一化为短名（`pkg`），
/// 与 `brew info --installed` 的 name 字段一致。
fn parse_outdated(json: &str) -> HashMap<String, (String, String)> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else {
        return HashMap::new();
    };
    let mut result = HashMap::new();
    for section in ["formulae", "casks"] {
        let Some(arr) = v.get(section).and_then(|v| v.as_array()) else {
            continue;
        };
        for item in arr {
            let name = item
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            let installed = item
                .get("installed_versions")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let current = item
                .get("current_version")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            // 归一化：`user/tap/pkg` → `pkg`（核心 formula/cask 无 `/` 不受影响）
            let short = name.rsplit('/').next().unwrap_or(name);
            result.insert(short.to_string(), (installed, current));
        }
    }
    result
}

/// 解析 brew info --json=v2 → name → PackageSummary（版本 + 描述）。
/// formulae 段用 "name" + "versions.stable"，casks 段用 "token" + "version"。
fn parse_summaries(json: &str, is_cask: bool) -> HashMap<String, PackageSummary> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else {
        return HashMap::new();
    };
    let section = if is_cask { "casks" } else { "formulae" };
    let name_field = if is_cask { "token" } else { "name" };
    let mut result = HashMap::new();
    if let Some(arr) = v.get(section).and_then(|v| v.as_array()) {
        for item in arr {
            let name = item
                .get(name_field)
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            let version = if is_cask {
                item.get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string()
            } else {
                item.get("versions")
                    .and_then(|v| v.get("stable"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string()
            };
            let desc = item
                .get("desc")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            result.insert(
                name.to_string(),
                PackageSummary {
                    name: name.to_string(),
                    version,
                    desc,
                },
            );
        }
    }
    result
}

/// 解析 `brew info --json=v2 --installed` → 已安装包列表（formula 在前、cask 在后，各自按名排序）。
/// formulae 段读 `installed[0].version`（已安装版本），casks 段读 `installed`（字符串）。
fn parse_installed(json: &str) -> Vec<RawPackage> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else {
        return vec![];
    };
    let mut formulae = vec![];
    let mut casks = vec![];

    if let Some(arr) = v.get("formulae").and_then(|v| v.as_array()) {
        for item in arr {
            let name = item
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            let version = item
                .get("installed")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.first())
                .and_then(|v| v.get("version"))
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            formulae.push(RawPackage {
                name: name.to_string(),
                kind: "formula".into(),
                desc: item
                    .get("desc")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                version,
            });
        }
    }

    if let Some(arr) = v.get("casks").and_then(|v| v.as_array()) {
        for item in arr {
            let name = item
                .get("token")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            casks.push(RawPackage {
                name: name.to_string(),
                kind: "cask".into(),
                desc: item
                    .get("desc")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                version: item
                    .get("installed")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
            });
        }
    }

    formulae.sort_by(|a, b| a.name.cmp(&b.name));
    casks.sort_by(|a, b| a.name.cmp(&b.name));
    formulae.extend(casks);
    formulae
}

/// 批量拉取包摘要（brew info --json=v2 读本地缓存，不联网）。
async fn fetch_summaries(
    brew: &str,
    path: &str,
    names: &[String],
    is_cask: bool,
) -> HashMap<String, PackageSummary> {
    if names.is_empty() {
        return HashMap::new();
    }
    let mut args: Vec<String> = vec!["info".into(), "--json=v2".into()];
    if is_cask {
        args.push("--cask".into());
    }
    args.extend(names.iter().cloned());
    let owned: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    brew_command(brew, path)
        .args(&owned)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .map(|o| parse_summaries(&String::from_utf8_lossy(&o.stdout), is_cask))
        .unwrap_or_default()
}

/// 获取 Homebrew 版本号（如 "4.4.0"，已去 "Homebrew " 前缀）。
async fn brew_version(brew: &str, path: &str) -> String {
    brew_command(brew, path)
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .next()
                .unwrap_or_default()
                .trim()
                .trim_start_matches("Homebrew ")
                .to_string()
        })
        .unwrap_or_default()
}

// ============================================================================
// 命令
// ============================================================================

#[tauri::command]
pub async fn brew_status(app: AppHandle) -> Result<BrewStatus, String> {
    let brew = brew_path().ok_or_else(|| "未找到 Homebrew（请先安装 brew）".to_string())?;
    let path = ensure_brew_path();

    // 并发：版本号 + 全部已安装包（名称/描述/已安装版本，一次取 formulae+casks）+ 过期检测
    let (version, installed, outdated) = tokio::join!(
        brew_version(brew, &path),
        async {
            brew_command(brew, &path)
                .args(["info", "--json=v2", "--installed"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .output()
                .await
                .map(|o| parse_installed(&String::from_utf8_lossy(&o.stdout)))
                .unwrap_or_default()
        },
        async {
            brew_command(brew, &path)
                .args(["outdated", "--json=v2"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .output()
                .await
                .map(|o| parse_outdated(&String::from_utf8_lossy(&o.stdout)))
                .unwrap_or_default()
        },
    );

    // 组装包列表（parse_installed 已按 formula→cask、各自名称排序）
    let mut packages = Vec::with_capacity(installed.len());
    for pkg in installed {
        let new_version = outdated
            .get(&pkg.name)
            .map(|(_, cur)| cur.clone())
            .unwrap_or_default();
        packages.push(InstalledPackage {
            name: pkg.name,
            kind: pkg.kind,
            desc: pkg.desc,
            version: pkg.version,
            new_version,
        });
    }

    let has_update = !outdated.is_empty();
    // 元数据陈旧则后台刷新（不阻塞本次返回，完成经 brew-run-done 驱动前端重拉）
    let refreshing = spawn_metadata_refresh(&app, brew, &path);

    Ok(BrewStatus {
        version,
        packages,
        has_update,
        refreshing,
    })
}

#[tauri::command]
pub async fn brew_services() -> Result<Vec<BrewService>, String> {
    let brew = brew_path().ok_or_else(|| "未找到 Homebrew".to_string())?;
    let path = ensure_brew_path();

    let output = brew_command(brew, &path)
        .args(["services", "list"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .map_err(|e| e.to_string())?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut services = vec![];
    for line in stdout.lines().skip(1) {
        // 格式: name status user
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }
        services.push(BrewService {
            name: parts[0].to_string(),
            status: parts.get(1).unwrap_or(&"unknown").to_string(),
        });
    }
    Ok(services)
}

#[tauri::command]
pub async fn brew_info(name: String) -> Result<BrewInfo, String> {
    let brew = brew_path().ok_or_else(|| "未找到 Homebrew".to_string())?;
    let path = ensure_brew_path();

    // 并发：依赖名 + 反向依赖名 + 目标详情（JSON，含 desc）
    let (deps_out, uses_out, info_out) = tokio::join!(
        brew_command(brew, &path)
            .args(["deps", &name])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output(),
        brew_command(brew, &path)
            .args(["uses", "--installed", &name])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output(),
        brew_command(brew, &path)
            .args(["info", "--json=v2", &name])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output(),
    );

    // 依赖名列表
    let dep_names: Vec<String> = deps_out
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect()
        })
        .unwrap_or_default();

    // 反向依赖名列表
    let use_names: Vec<String> = uses_out
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect()
        })
        .unwrap_or_default();

    // desc：从 JSON 解析（formulae + casks 双段查找，自动检测包类型）
    let desc = info_out
        .map(|o| {
            let json = String::from_utf8_lossy(&o.stdout);
            parse_summaries(&json, false)
                .get(&name)
                .map(|s| s.desc.clone())
                .unwrap_or_else(|| {
                    parse_summaries(&json, true)
                        .get(&name)
                        .map(|s| s.desc.clone())
                        .unwrap_or_default()
                })
        })
        .unwrap_or_default();

    // 并发 enrich 依赖 + 被依赖的版本和描述
    let all_names: Vec<String> = dep_names.iter().chain(use_names.iter()).cloned().collect();
    let summaries = fetch_summaries(brew, &path, &all_names, false).await;

    let deps: Vec<PackageSummary> = dep_names
        .iter()
        .map(|n| {
            summaries.get(n).cloned().unwrap_or(PackageSummary {
                name: n.clone(),
                version: String::new(),
                desc: String::new(),
            })
        })
        .collect();
    let uses: Vec<PackageSummary> = use_names
        .iter()
        .map(|n| {
            summaries.get(n).cloned().unwrap_or(PackageSummary {
                name: n.clone(),
                version: String::new(),
                desc: String::new(),
            })
        })
        .collect();

    Ok(BrewInfo { desc, deps, uses })
}

#[tauri::command]
pub async fn brew_run_state() -> Result<Option<BrewRunState>, String> {
    Ok(BREW_RUNNING.lock().unwrap().clone())
}

#[tauri::command]
pub async fn brew_run(
    app: AppHandle,
    operation: String,
    target: Option<String>,
    on_event: Channel<BrewEvent>,
) -> Result<(), String> {
    let brew = brew_path().ok_or_else(|| "未找到 Homebrew".to_string())?;
    let path = ensure_brew_path();

    // 每步操作：(label, args) — args 用 owned String 以支持动态 target
    let steps: Vec<(&str, Vec<String>)> = match operation.as_str() {
        "update_upgrade" => vec![
            ("update", vec!["update".into()]),
            ("upgrade", vec!["upgrade".into()]),
            ("cleanup", vec!["cleanup".into()]),
            ("autoremove", vec!["autoremove".into()]),
        ],
        "uninstall" => {
            let name = target.ok_or("卸载需要包名")?;
            vec![
                ("uninstall", vec!["uninstall".into(), name]),
                ("autoremove", vec!["autoremove".into()]),
            ]
        }
        "autoremove" => vec![("autoremove", vec!["autoremove".into()])],
        "services_start" => {
            let name = target.ok_or("启动服务需要包名")?;
            vec![(
                "services start",
                vec!["services".into(), "start".into(), name],
            )]
        }
        "services_stop" => {
            let name = target.ok_or("停止服务需要包名")?;
            vec![(
                "services stop",
                vec!["services".into(), "stop".into(), name],
            )]
        }
        "services_restart" => {
            let name = target.ok_or("重启服务需要包名")?;
            vec![(
                "services restart",
                vec!["services".into(), "restart".into(), name],
            )]
        }
        _ => return Err(format!("未知操作: {operation}")),
    };

    // RAII guard：占用全局运行态，Drop 时自动清理 + emit brew-run-done。
    // 防并发：已有操作进行中则拒绝（窗口隐藏重开后前端不能再触发第二个）。
    let Some(guard) = RunGuard::try_acquire(app, &operation) else {
        return Err("已有 Homebrew 操作正在运行".to_string());
    };

    for (label, args) in &steps {
        guard.set_step(label);
        let _ = on_event.send(BrewEvent {
            kind: "step",
            text: label.to_string(),
        });
        let owned_args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let success = run_brew_step(brew, &path, &owned_args, &on_event).await?;
        if !success {
            let _ = on_event.send(BrewEvent {
                kind: "error",
                text: format!("brew {label} 失败"),
            });
            return Err(format!("brew {label} 失败"));
        }
    }

    let _ = on_event.send(BrewEvent {
        kind: "done",
        text: "完成".to_string(),
    });
    Ok(())
    // guard drops → BREW_RUNNING 清空 + emit brew-run-done
}

/// 流式执行单个 brew 子命令，stdout + stderr 逐行经 Channel 回传。
async fn run_brew_step(
    brew: &str,
    path: &str,
    args: &[&str],
    on_event: &Channel<BrewEvent>,
) -> Result<bool, String> {
    let mut child = brew_command(brew, path)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("启动 brew 失败: {e}"))?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    // mpsc 合流 stdout + stderr（两 reader task 各持一份 tx clone）
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    let tx1 = tx.clone();
    tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = tx1.send(line);
        }
    });

    let tx2 = tx.clone();
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = tx2.send(line);
        }
    });

    drop(tx);

    // 逐行转发（reader task 结束 = 管道 EOF = 子进程已退出）
    while let Some(line) = rx.recv().await {
        let _ = on_event.send(BrewEvent {
            kind: "line",
            text: line,
        });
    }

    let status = child.wait().await.map_err(|e| e.to_string())?;
    Ok(status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_outdated ──

    #[test]
    fn parse_outdated_formula_and_cask() {
        let json = r#"{
            "formulae": [
                {"name": "git", "installed_versions": "2.40.0", "current_version": "2.43.0"},
                {"name": "curl", "installed_versions": "8.4.0", "current_version": "8.5.0"}
            ],
            "casks": [
                {"name": "firefox", "installed_versions": "120.0", "current_version": "121.0"}
            ]
        }"#;
        let map = parse_outdated(json);
        assert_eq!(map.get("git").unwrap(), &("2.40.0".into(), "2.43.0".into()));
        assert_eq!(map.get("curl").unwrap(), &("8.4.0".into(), "8.5.0".into()));
        assert_eq!(
            map.get("firefox").unwrap(),
            &("120.0".into(), "121.0".into())
        );
    }

    #[test]
    fn parse_outdated_empty_json() {
        let map = parse_outdated(r#"{"formulae":[],"casks":[]}"#);
        assert!(map.is_empty());
    }

    #[test]
    fn parse_outdated_invalid_json() {
        assert!(parse_outdated("not json").is_empty());
    }

    #[test]
    fn parse_outdated_skips_empty_name() {
        let json =
            r#"{"formulae":[{"name":"","installed_versions":"1.0","current_version":"2.0"}]}"#;
        assert!(parse_outdated(json).is_empty());
    }

    #[test]
    fn parse_outdated_normalizes_tap_full_name() {
        // 第三方 tap formula 的 name 是全限定名（user/tap/pkg），应归一化为短名
        let json = r#"{
            "formulae": [
                {"name": "anomalyco/tap/opencode", "installed_versions": "1.18.15", "current_version": "1.18.16"}
            ]
        }"#;
        let map = parse_outdated(json);
        assert_eq!(
            map.get("opencode").unwrap(),
            &("1.18.15".into(), "1.18.16".into())
        );
        // 全限定名不应残留在 map 中
        assert!(map.get("anomalyco/tap/opencode").is_none());
    }

    // ── parse_summaries ──

    #[test]
    fn parse_summaries_formula() {
        let json = r#"{
            "formulae": [
                {"name": "git", "desc": "Distributed version control", "versions": {"stable": "2.43.0"}},
                {"name": "curl", "desc": "Get a file from HTTP", "versions": {"stable": "8.5.0"}}
            ]
        }"#;
        let map = parse_summaries(json, false);
        let git = map.get("git").unwrap();
        assert_eq!(git.version, "2.43.0");
        assert_eq!(git.desc, "Distributed version control");
    }

    #[test]
    fn parse_summaries_cask_uses_token_and_version() {
        let json = r#"{
            "casks": [
                {"token": "firefox", "desc": "Web browser", "version": "121.0"}
            ]
        }"#;
        let map = parse_summaries(json, true);
        let ff = map.get("firefox").unwrap();
        assert_eq!(ff.version, "121.0");
        assert_eq!(ff.desc, "Web browser");
    }

    #[test]
    fn parse_summaries_missing_desc_defaults_empty() {
        let json = r#"{"formulae":[{"name":"foo","versions":{"stable":"1.0"}}]}"#;
        let map = parse_summaries(json, false);
        assert_eq!(map.get("foo").unwrap().desc, "");
    }

    #[test]
    fn parse_summaries_empty_name_skipped() {
        let json = r#"{"formulae":[{"name":"","desc":"x","versions":{"stable":"1.0"}}]}"#;
        let map = parse_summaries(json, false);
        assert!(map.is_empty());
    }

    // ── parse_installed ──

    #[test]
    fn parse_installed_formula_and_cask() {
        let json = r#"{
            "formulae": [
                {"name": "git", "desc": "Distributed VCS", "versions": {"stable": "2.43.0"}, "installed": [{"version": "2.40.0"}]},
                {"name": "curl", "desc": "Get a file from HTTP", "versions": {"stable": "8.5.0"}, "installed": [{"version": "8.5.0"}]}
            ],
            "casks": [
                {"token": "firefox", "desc": "Web browser", "version": "121.0", "installed": "120.0"}
            ]
        }"#;
        let pkgs = parse_installed(json);
        assert_eq!(pkgs.len(), 3);
        // formulae 在前、各自按名排序
        assert_eq!(pkgs[0].name, "curl");
        assert_eq!(pkgs[0].kind, "formula");
        assert_eq!(pkgs[0].version, "8.5.0");
        assert_eq!(pkgs[1].name, "git");
        assert_eq!(pkgs[1].version, "2.40.0");
        assert_eq!(pkgs[1].desc, "Distributed VCS");
        // casks 在后
        assert_eq!(pkgs[2].name, "firefox");
        assert_eq!(pkgs[2].kind, "cask");
        assert_eq!(pkgs[2].version, "120.0");
    }

    #[test]
    fn parse_installed_empty_json() {
        assert!(parse_installed(r#"{"formulae":[],"casks":[]}"#).is_empty());
    }

    #[test]
    fn parse_installed_invalid_json() {
        assert!(parse_installed("not json").is_empty());
    }

    #[test]
    fn parse_installed_missing_installed_defaults_empty() {
        let json = r#"{"formulae":[{"name":"foo","desc":"x"}]}"#;
        let pkgs = parse_installed(json);
        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].version, "");
    }

    #[test]
    fn parse_installed_cask_null_installed() {
        // 部分旧版 Homebrew cask 的 installed 字段可能为 null
        let json = r#"{"casks":[{"token":"firefox","desc":"Web browser","installed":null}]}"#;
        let pkgs = parse_installed(json);
        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].version, "");
    }

    // ── metadata_stale_since ──

    #[test]
    fn metadata_stale_since_none_is_stale() {
        // 从未 update（无 api 缓存）→ 需要刷新
        assert!(metadata_stale_since(None));
    }

    #[test]
    fn metadata_stale_since_recent_is_fresh() {
        let now = SystemTime::now();
        assert!(!metadata_stale_since(Some(now)));
        assert!(!metadata_stale_since(
            now.checked_sub(Duration::from_secs(3600))
        ));
    }

    #[test]
    fn metadata_stale_since_old_is_stale() {
        let now = SystemTime::now();
        assert!(metadata_stale_since(
            now.checked_sub(Duration::from_secs(24 * 3600 + 60))
        ));
    }

    #[test]
    fn metadata_stale_since_future_is_stale() {
        // 时钟异常（mtime 在未来，elapsed Err）→ 强制刷新防节流卡死
        let now = SystemTime::now();
        assert!(metadata_stale_since(
            now.checked_add(Duration::from_secs(3600))
        ));
    }

    // ── 刷新失败冷却（防 done→重拉→再 spawn 无限环）──

    #[test]
    fn refresh_cooldown_blocks_rapid_retry() {
        *LAST_REFRESH_ATTEMPT.lock().unwrap() = None;
        // 从未发起 → 放行
        assert!(!refresh_in_cooldown());
        note_refresh_attempt();
        // 刚发起 → 冷却内拦截
        assert!(refresh_in_cooldown());
        // 冷却过期 → 放行
        *LAST_REFRESH_ATTEMPT.lock().unwrap() =
            Some(Instant::now() - REFRESH_RETRY_COOLDOWN - Duration::from_secs(1));
        assert!(!refresh_in_cooldown());
        *LAST_REFRESH_ATTEMPT.lock().unwrap() = None; // 清零防影响其他测试
    }

    // ── BREW_RUNNING 并发防互斥 ──

    #[test]
    fn try_set_running_rejects_concurrent() {
        // 清零（其余测试均为纯 JSON 解析，不触碰 BREW_RUNNING，无竞态）
        *BREW_RUNNING.lock().unwrap() = None;

        assert!(try_set_running("update_upgrade"));
        // 第二次占用应被拒绝
        assert!(!try_set_running("uninstall"));

        let state = take_running().unwrap();
        assert_eq!(state.operation, "update_upgrade");
        assert_eq!(state.step, "");

        // take 后恢复空闲
        assert!(BREW_RUNNING.lock().unwrap().is_none());
        // 空闲后可再次占用
        assert!(try_set_running("services_start"));
        take_running();
    }
}
