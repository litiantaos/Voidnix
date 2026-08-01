//! Homebrew 管理扩展：包列表 + 服务管理 + 包详情 + 一键更新 + 卸载。
//!
//! 命令：
//! - `brew_status`：Homebrew 版本 + 全部已安装包（版本 / 最新版 / 描述 / 是否可升级）
//! - `brew_services`：列出 brew services 状态
//! - `brew_info`：单个包详情（依赖 / 反向依赖 / 版本 / 描述）
//! - `brew_run`：流式执行 brew 子命令（update→upgrade→cleanup→autoremove / uninstall→autoremove / services start|stop|restart）
//!
//! brew 可执行路径硬探测（/opt/homebrew/bin/brew Apple Silicon / /usr/local/bin/brew Intel），
//! 不依赖 GUI 进程 PATH（可能不含 brew bin 目录）。

use crate::runtime::registry::Extension;
use std::collections::HashMap;
use std::process::Stdio;
use tauri::ipc::Channel;
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

#[derive(serde::Serialize)]
pub struct BrewStatus {
    pub version: String,
    pub packages: Vec<InstalledPackage>,
    pub has_update: bool,
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

/// 解析 brew outdated --json=v2 输出 → name → (installed, current) 映射。
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
            result.insert(name.to_string(), (installed, current));
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
    Command::new(brew)
        .args(&owned)
        .env("PATH", path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .map(|o| parse_summaries(&String::from_utf8_lossy(&o.stdout), is_cask))
        .unwrap_or_default()
}

/// 获取 Homebrew 版本号（如 "4.4.0"，已去 "Homebrew " 前缀）。
async fn brew_version(brew: &str, path: &str) -> String {
    Command::new(brew)
        .arg("--version")
        .env("PATH", path)
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

/// 扫描 brew list --<flag> --versions 输出，返回 (name, version) 列表（按 name 排序）。
async fn list_with_versions(brew: &str, path: &str, flag: &str) -> Vec<(String, String)> {
    let output = Command::new(brew)
        .args(["list", flag, "--versions"])
        .env("PATH", path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await;
    let mut result = vec![];
    if let Ok(o) = output {
        for line in String::from_utf8_lossy(&o.stdout).lines() {
            let mut parts = line.split_whitespace();
            let name = parts.next().unwrap_or_default();
            let version = parts.next().unwrap_or_default();
            if !name.is_empty() && !name.starts_with("==") {
                result.push((name.to_string(), version.to_string()));
            }
        }
    }
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}

// ============================================================================
// 命令
// ============================================================================

#[tauri::command]
pub async fn brew_status() -> Result<BrewStatus, String> {
    let brew = brew_path().ok_or_else(|| "未找到 Homebrew（请先安装 brew）".to_string())?;
    let path = ensure_brew_path();

    // 并发：版本号 + 包名+版本（formula + cask）+ 过期检测
    let (version, formula_pairs, cask_pairs, outdated) = tokio::join!(
        brew_version(brew, &path),
        list_with_versions(brew, &path, "--formula"),
        list_with_versions(brew, &path, "--cask"),
        async {
            Command::new(brew)
                .args(["outdated", "--json=v2"])
                .env("PATH", &path)
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .output()
                .await
                .map(|o| parse_outdated(&String::from_utf8_lossy(&o.stdout)))
                .unwrap_or_default()
        },
    );

    // 并发批量拉取摘要（版本 + 描述）
    let formula_names: Vec<String> = formula_pairs.iter().map(|(n, _)| n.clone()).collect();
    let cask_names: Vec<String> = cask_pairs.iter().map(|(n, _)| n.clone()).collect();
    let (formula_summaries, cask_summaries) = tokio::join!(
        fetch_summaries(brew, &path, &formula_names, false),
        fetch_summaries(brew, &path, &cask_names, true),
    );

    // 组装包列表（formula 在前、cask 在后，各自已按名称排序）
    let mut packages = Vec::with_capacity(formula_pairs.len() + cask_pairs.len());
    for (pairs, kind, summaries) in [
        (&formula_pairs, "formula", &formula_summaries),
        (&cask_pairs, "cask", &cask_summaries),
    ] {
        for (name, ver) in pairs {
            let new_version = outdated
                .get(name)
                .map(|(_, cur)| cur.clone())
                .unwrap_or_default();
            packages.push(InstalledPackage {
                desc: summaries
                    .get(name)
                    .map(|s| s.desc.clone())
                    .unwrap_or_default(),
                version: ver.clone(),
                new_version,
                name: name.clone(),
                kind: kind.to_string(),
            });
        }
    }

    let has_update = !outdated.is_empty();

    Ok(BrewStatus {
        version,
        packages,
        has_update,
    })
}

#[tauri::command]
pub async fn brew_services() -> Result<Vec<BrewService>, String> {
    let brew = brew_path().ok_or_else(|| "未找到 Homebrew".to_string())?;
    let path = ensure_brew_path();

    let output = Command::new(brew)
        .args(["services", "list"])
        .env("PATH", &path)
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
        Command::new(brew)
            .args(["deps", &name])
            .env("PATH", &path)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output(),
        Command::new(brew)
            .args(["uses", "--installed", &name])
            .env("PATH", &path)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output(),
        Command::new(brew)
            .args(["info", "--json=v2", &name])
            .env("PATH", &path)
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
pub async fn brew_run(
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

    for (label, args) in &steps {
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
}

/// 流式执行单个 brew 子命令，stdout + stderr 逐行经 Channel 回传。
async fn run_brew_step(
    brew: &str,
    path: &str,
    args: &[&str],
    on_event: &Channel<BrewEvent>,
) -> Result<bool, String> {
    let mut child = Command::new(brew)
        .args(args)
        .env("PATH", path)
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
}
