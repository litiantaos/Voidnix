use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::atomic::Ordering;

use serde::Serialize;

use super::cache::{get_cached_apps, get_cached_files};
use super::types::{SearchResult, SEARCH_SESSION};

/// 基于 path 的稳定 hash id（同 path 同 id，进程内确定性）。
/// 用于 dedupe key 与 Vue :key 稳定化——缓存重建/mdfind 重查时同 path 保持同 key，DOM 复用而非重建。
fn path_hash(path: &str) -> String {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

/// 返回全量应用元数据（不含图标），由前端做拼音匹配与排序。
/// 图标单独走 get_app_icons 批量拉取，避免 ~600KB base64 膨胀搜索热路径 IPC。
#[tauri::command]
pub async fn search_apps() -> Result<Vec<SearchResult>, String> {
    let apps = get_cached_apps().await;
    // 锁内直接点查，避免每轮 clone 整个 HashMap（deltas 仅记录本次会话启动过的 app 路径）
    let session_deltas = SEARCH_SESSION.session_use_deltas.lock().ok();

    let results: Vec<SearchResult> = apps
        .iter()
        .map(|app| {
            let delta = session_deltas
                .as_ref()
                .and_then(|m| m.get(&app.path).copied())
                .unwrap_or(0);
            let count = app.use_count.load(Ordering::Relaxed) + delta;
            SearchResult {
                id: format!("app-{}", path_hash(&app.path)),
                title: app.name.clone(),
                path: app.path.clone(),
                kind: "application".to_string(),
                icon: None,
                last_used: app.last_used.clone(),
                score: None,
                use_count: Some(count),
                parent: None,
            }
        })
        .collect();

    Ok(results)
}

/// 批量返回应用图标（id → base64），供前端异步补全、与元数据解耦。
#[derive(Serialize)]
pub struct AppIcon {
    pub id: String,
    pub icon: Option<String>,
}

#[tauri::command]
pub async fn get_app_icons() -> Result<Vec<AppIcon>, String> {
    let apps = get_cached_apps().await;
    let icons: Vec<AppIcon> = apps
        .iter()
        .map(|app| AppIcon {
            id: format!("app-{}", path_hash(&app.path)),
            icon: app.icon_cache.clone(),
        })
        .collect();
    Ok(icons)
}

/// 内存文件搜索：对预建 FILE_CACHE 做 substring + 拼音匹配 + 复合打分（名称 + 频率 + 近期），
/// 返回 top 100 候选。典型 ~3ms（20k 条目），不再 spawn mdfind 子进程。
/// ASCII 查询额外匹配 CJK 文件名的拼音键（首字母+全拼），召回拼音命中（如 "sjwd"→"设计文档"）。
/// 前端 scoreFields 再做 fuzzy + boost 精排。
#[tauri::command]
pub async fn search_files(query: String) -> Result<Vec<SearchResult>, String> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(vec![]);
    }
    let q_lower = q.to_lowercase();
    let is_ascii = q_lower.is_ascii();

    let cache = get_cached_files().await;
    let session_deltas = SEARCH_SESSION.session_use_deltas.lock().ok();

    let now_hours = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64 / 3600)
        .unwrap_or(0);

    // 复合打分：name substring(前缀 1000 / 包含 600) + pinyin(300) + frequency(log2, cap 1500)
    //          + recency(<1h=300 / <24h=200 / <168h=100 / <720h=50) + folder 优先 240
    let mut candidates: Vec<(usize, i32)> = cache
        .iter()
        .enumerate()
        .filter_map(|(i, f)| {
            // 名称 substring 匹配
            let name_score = f.name_lower.find(&q_lower).map(|idx| {
                let base = if idx == 0 { 1000 } else { 600 };
                base - idx as i32 * 4
            });

            // 拼音匹配（ASCII 查询 + CJK 文件名）：首字母或全拼 substring 命中
            let pinyin_score = if is_ascii && !f.pinyin_key.is_empty() {
                if f.pinyin_key.contains(q_lower.as_str()) {
                    Some(300)
                } else {
                    None
                }
            } else {
                None
            };

            let base = name_score.max(pinyin_score)?;

            let delta = session_deltas
                .as_ref()
                .and_then(|m| m.get(&f.path).copied())
                .unwrap_or(0);
            let total_use = f.use_count + delta;

            let freq = if total_use == 0 {
                0
            } else {
                let s = ((total_use as f64 + 1.0).ln() / std::f64::consts::LN_2 * 150.0) as i32;
                s.min(1500)
            };

            let recency = if f.last_used_hours > 0 {
                let hours_ago = now_hours - f.last_used_hours;
                if hours_ago < 1 {
                    300
                } else if hours_ago < 24 {
                    200
                } else if hours_ago < 168 {
                    100
                } else if hours_ago < 720 {
                    50
                } else {
                    0
                }
            } else {
                0
            };

            let folder_bonus = if f.is_folder { 240 } else { 0 };

            Some((i, base + freq + recency + folder_bonus))
        })
        .collect();

    candidates.sort_unstable_by_key(|(_, score)| std::cmp::Reverse(*score));
    candidates.truncate(100);

    let results: Vec<SearchResult> = candidates
        .iter()
        .map(|(i, _)| {
            let f = &cache[*i];
            let delta = session_deltas
                .as_ref()
                .and_then(|m| m.get(&f.path).copied())
                .unwrap_or(0);
            let kind_str = if f.is_folder { "folder" } else { "file" };
            SearchResult {
                id: format!("{}-{}", kind_str, path_hash(&f.path)),
                title: f.name.clone(),
                path: f.path.clone(),
                kind: kind_str.to_string(),
                icon: None,
                last_used: f.last_used.clone(),
                score: None,
                use_count: Some(f.use_count + delta),
                parent: f.parent.clone(),
            }
        })
        .collect();

    Ok(results)
}

#[tauri::command]
pub async fn reveal_in_finder(path: String) -> Result<(), String> {
    let p = Path::new(&path);
    if !p.is_absolute() {
        return Err(format!("Path is not absolute: {}", path));
    }
    if !p.exists() {
        return Err(format!("Path does not exist: {}", path));
    }

    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::NSWorkspace;
        use objc2_foundation::NSString;
        let ns_path = NSString::from_str(&path);
        let ws = NSWorkspace::sharedWorkspace();
        let _ = ws.selectFile_inFileViewerRootedAtPath(Some(&ns_path), &NSString::from_str(""));
    }

    Ok(())
}

#[tauri::command]
pub async fn launch_app(path: String) -> Result<(), String> {
    let p = Path::new(&path);
    if !p.is_absolute() {
        return Err(format!("Path is not absolute: {}", path));
    }
    if !p.exists() {
        return Err(format!("Path does not exist: {}", path));
    }

    tokio::process::Command::new("open")
        .arg(&path)
        .spawn()
        .map_err(|e| format!("Failed to launch: {e}"))?;

    SEARCH_SESSION.increment_use_count(&path);

    Ok(())
}

/// 路径元数据（信息面板按需拉取，不污染搜索热路径）。
#[derive(Debug, Serialize)]
pub struct PathMetadata {
    pub size: Option<u64>,
    pub created: Option<String>,
    pub modified: Option<String>,
    pub last_used: Option<String>,
    pub version: Option<String>,
}

/// 解析 .app 的 Info.plist 取版本号（CFBundleShortVersionString → CFBundleVersion 兜底）。
fn get_app_version(app_path: &str) -> Option<String> {
    let plist_path = Path::new(app_path).join("Contents").join("Info.plist");
    let dict = plist::Value::from_file(&plist_path)
        .ok()?
        .into_dictionary()?;
    let v = dict
        .get("CFBundleShortVersionString")
        .and_then(|v| v.as_string())
        .or_else(|| dict.get("CFBundleVersion").and_then(|v| v.as_string()))?;
    let s = v.trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// 一次 mdls 拉大小/创建/修改/上次打开；size 未命中且普通文件时兜底 std::fs 元数据。
fn fetch_metadata(path: &str, is_app: bool) -> PathMetadata {
    let mut size: Option<u64> = None;
    let mut created: Option<String> = None;
    let mut modified: Option<String> = None;
    let mut last_used: Option<String> = None;

    let out = std::process::Command::new("mdls")
        .arg("-name")
        .arg("kMDItemFSSize")
        .arg("-name")
        .arg("kMDItemDateAdded")
        .arg("-name")
        .arg("kMDItemContentModificationDate")
        .arg("-name")
        .arg("kMDItemLastUsedDate")
        .arg(path)
        .output();
    if let Ok(o) = out {
        for line in String::from_utf8_lossy(&o.stdout).lines() {
            if let Some((k, v)) = line.split_once('=') {
                let val = v.trim().trim_matches('"');
                if val.is_empty() || val == "(null)" {
                    continue;
                }
                match k.trim() {
                    "kMDItemFSSize" => size = val.parse::<u64>().ok(),
                    "kMDItemDateAdded" => created = Some(val.to_string()),
                    "kMDItemContentModificationDate" => modified = Some(val.to_string()),
                    "kMDItemLastUsedDate" => last_used = Some(val.to_string()),
                    _ => {}
                }
            }
        }
    }

    if size.is_none() {
        if let Ok(meta) = std::fs::metadata(path) {
            if meta.is_file() {
                size = Some(meta.len());
            }
        }
    }

    let version = if is_app { get_app_version(path) } else { None };

    PathMetadata {
        size,
        created,
        modified,
        last_used,
        version,
    }
}

/// 拉单条路径元数据（信息面板消费）。mdls 是同步阻塞子进程，spawn_blocking 避免阻塞异步运行时。
#[tauri::command]
pub async fn get_path_metadata(path: String) -> Result<PathMetadata, String> {
    let p = Path::new(&path);
    if !p.is_absolute() {
        return Err(format!("Path is not absolute: {}", path));
    }
    if !p.exists() {
        return Err(format!("Path does not exist: {}", path));
    }
    let is_app = path.ends_with(".app");
    tokio::task::spawn_blocking(move || fetch_metadata(&path, is_app))
        .await
        .map_err(|e| format!("{e}"))
}
