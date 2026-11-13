use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const APP_SEARCH_DIRS: &[&str] = &[
    "/Applications",
    "/System/Applications",
    "/System/Library/CoreServices/Applications",
];

/// 应用查询根目录（含存在时追加的 ~/Applications）。
fn app_query_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = APP_SEARCH_DIRS.iter().map(PathBuf::from).collect();
    if let Some(home) = dirs::home_dir() {
        let home_apps = home.join("Applications");
        if home_apps.exists() {
            dirs.push(home_apps);
        }
    }
    dirs
}

fn get_app_name_from_plist(app_path: &str) -> Option<String> {
    let plist_path = Path::new(app_path).join("Contents").join("Info.plist");
    let dict = plist::Value::from_file(&plist_path)
        .ok()?
        .into_dictionary()?;
    let name = dict
        .get("CFBundleDisplayName")
        .and_then(|v| v.as_string())
        .or_else(|| dict.get("CFBundleName").and_then(|v| v.as_string()))?;
    let name = name.to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// fallback 路径专用：plist 直读应用名（~1ms），失败回退文件名。
/// 不走 mdls 子进程（~50-100ms/个），消除启动期 fallback 风暴；
/// usage 由独立索引按 path 提供，不在 fallback 条目上重复查询。
fn name_from_plist_or_fallback(app_path: &str) -> String {
    get_app_name_from_plist(app_path).unwrap_or_else(|| {
        Path::new(app_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_string()
    })
}

pub(super) fn scan_apps_from_dir(dir: &Path) -> Vec<String> {
    scan_apps_from_dir_depth(dir, 0)
}

fn scan_apps_from_dir_depth(dir: &Path, depth: u32) -> Vec<String> {
    if depth > 5 {
        return Vec::new();
    }
    let mut apps = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "app") {
                if let Some(path_str) = path.to_str() {
                    apps.push(path_str.to_string());
                }
            } else if path.is_dir() {
                apps.extend(scan_apps_from_dir_depth(&path, depth + 1));
            }
        }
    }
    apps
}

pub(super) fn parse_mdfind_attr_output(stdout: &str) -> Vec<(String, String, Option<String>, u32)> {
    let mut results: Vec<(String, String, Option<String>, u32)> = Vec::new();
    let mut current_path: Option<String> = None;
    let mut current_name = String::new();
    let mut current_last_used: Option<String> = None;
    let mut current_use_count: u32 = 0;

    let mut flush = |path: &mut Option<String>,
                     name: &mut String,
                     last_used: &mut Option<String>,
                     use_count: &mut u32| {
        if let Some(p) = path.take() {
            if name.is_empty() || name == "(null)" {
                *name = std::path::Path::new(&p)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Unknown")
                    .to_string();
            }
            results.push((
                p,
                std::mem::take(name),
                last_used.take(),
                std::mem::take(use_count),
            ));
        }
    };

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            flush(
                &mut current_path,
                &mut current_name,
                &mut current_last_used,
                &mut current_use_count,
            );
            continue;
        }

        if trimmed.starts_with('/') {
            flush(
                &mut current_path,
                &mut current_name,
                &mut current_last_used,
                &mut current_use_count,
            );

            let parts: Vec<&str> = trimmed.splitn(4, "   ").collect();
            current_path = Some(parts[0].trim().to_string());

            for part in &parts[1..] {
                let part = part.trim();
                if part.starts_with("kMDItemDisplayName") {
                    if let Some(val) = part.split('=').nth(1) {
                        let cleaned = val.trim().trim_matches('"').to_string();
                        current_name = if cleaned.ends_with(".app") {
                            cleaned.strip_suffix(".app").unwrap_or(&cleaned).to_string()
                        } else {
                            cleaned
                        };
                    }
                } else if part.starts_with("kMDItemLastUsedDate") {
                    if let Some(val) = part.split('=').nth(1) {
                        let cleaned = val.trim().to_string();
                        if cleaned != "(null)" {
                            current_last_used = Some(cleaned);
                        }
                    }
                } else if part.starts_with("kMDItemUseCount") {
                    if let Some(val) = part.split('=').nth(1) {
                        if let Ok(count) = val.trim().parse::<u32>() {
                            current_use_count = count;
                        }
                    }
                }
            }
        } else if trimmed.starts_with("kMDItemDisplayName") {
            if let Some(val) = trimmed.split('=').nth(1) {
                let cleaned = val.trim().trim_matches('"').to_string();
                current_name = if cleaned.ends_with(".app") {
                    cleaned.strip_suffix(".app").unwrap_or(&cleaned).to_string()
                } else {
                    cleaned
                };
            }
        } else if trimmed.starts_with("kMDItemLastUsedDate") {
            if let Some(val) = trimmed.split('=').nth(1) {
                let cleaned = val.trim().to_string();
                if cleaned != "(null)" {
                    current_last_used = Some(cleaned);
                }
            }
        } else if trimmed.starts_with("kMDItemUseCount") {
            if let Some(val) = trimmed.split('=').nth(1) {
                if let Ok(count) = val.trim().parse::<u32>() {
                    current_use_count = count;
                }
            }
        }
    }
    flush(
        &mut current_path,
        &mut current_name,
        &mut current_last_used,
        &mut current_use_count,
    );

    results
}

/// 轻量探测：Spotlight 是否已就绪（mdfind 能否返回应用条目）。
/// 冷启动自愈重试消费（见 cache.rs retry_mdfind_rebuild），不解析属性、只数命中数。
pub(super) fn mdfind_app_count() -> usize {
    let mut command = std::process::Command::new("mdfind");
    command.arg("kMDItemContentType == 'com.apple.application-bundle'");
    command.arg("-onlyin").arg("/Applications");
    super::mdfind::run(command, Duration::from_secs(5))
        .map(|output| output.lines().filter(|l| l.trim().starts_with('/')).count())
        .unwrap_or(0)
}

/// 应用元数据条目：(path, name)。使用频次/近因由 usage 索引独立维护。
pub(super) type AppMeta = (String, String);

/// 收集全量应用静态元数据（path + name）。返回 (条目, mdfind 命中数)：mdfind_count == 0
/// 表示 Spotlight 未就绪（开机后首次查询等），条目为 fs 兜底扫描产出（含系统
/// CoreServices 组件），调用方据此调度自愈重试。
pub(super) fn collect_apps_with_metadata() -> (Vec<AppMeta>, usize) {
    let mut command = std::process::Command::new("mdfind");
    command.arg("kMDItemContentType == 'com.apple.application-bundle'");
    command.arg("-attr").arg("kMDItemDisplayName");
    for dir in app_query_dirs() {
        command.arg("-onlyin").arg(dir);
    }

    let mut results: Vec<AppMeta> = super::mdfind::run(command, Duration::from_secs(10))
        .map(|stdout| {
            parse_mdfind_attr_output(&stdout)
                .into_iter()
                .map(|(path, name, _, _)| (path, name))
                .collect()
        })
        .unwrap_or_default();
    let mdfind_count = results.len();

    let existing: std::collections::HashSet<String> =
        results.iter().map(|(p, _)| p.clone()).collect();

    let mut fs_apps: Vec<String> = Vec::new();
    for dir in APP_SEARCH_DIRS {
        fs_apps.extend(scan_apps_from_dir(Path::new(dir)));
    }
    if let Some(home_dir) = dirs::home_dir() {
        let home_apps = home_dir.join("Applications");
        if home_apps.exists() {
            fs_apps.extend(scan_apps_from_dir(&home_apps));
        }
    }

    for path in fs_apps {
        if !existing.contains(&path) {
            log::info!("Found app via filesystem scan (not in mdfind): {}", path);
            let name = name_from_plist_or_fallback(&path);
            results.push((path, name));
        }
    }

    let finder_path = "/System/Library/CoreServices/Finder.app".to_string();
    if Path::new(&finder_path).exists() && !existing.contains(&finder_path) {
        let name = name_from_plist_or_fallback(&finder_path);
        results.push((finder_path, name));
    }

    (results, mdfind_count)
}

/// 一次 mdfind 拉应用目录下 use_count>0 的使用元数据（path → (use_count, last_used)）。
/// 与 `collect_apps_with_metadata` 共用同一批根目录；只取使用属性，不拉名称/图标。
/// 返回 None 表示 mdfind 失败/超时，调用方保留旧快照（不把临时故障当成「全部零使用」）。
pub(super) fn query_app_usage() -> Option<HashMap<String, (u32, Option<String>)>> {
    let mut command = std::process::Command::new("mdfind");
    command.arg("kMDItemContentType == 'com.apple.application-bundle'");
    command.arg("-attr").arg("kMDItemUseCount");
    command.arg("-attr").arg("kMDItemLastUsedDate");
    for dir in app_query_dirs() {
        command.arg("-onlyin").arg(dir);
    }

    let output = super::mdfind::run(command, Duration::from_secs(10))?;
    let parsed = parse_mdfind_attr_output(&output);
    Some(
        parsed
            .into_iter()
            .filter_map(|(path, _, last_used, use_count)| {
                (use_count > 0 || last_used.is_some()).then_some((path, (use_count, last_used)))
            })
            .collect(),
    )
}
