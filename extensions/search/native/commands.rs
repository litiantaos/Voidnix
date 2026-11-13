use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

use serde::Serialize;

use super::cache::{get_cached_apps, get_cached_files};
use super::types::SearchResult;
use super::usage;

/// 基于 path 的稳定 hash id（同 path 同 id，进程内确定性）。
/// 用于 dedupe key 与 Vue :key 稳定化——缓存重建/mdfind 重查时同 path 保持同 key，DOM 复用而非重建。
fn path_hash(path: &str) -> String {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

/// 窗口获焦时的轻量 tick：TTL 到期则后台刷新 usage，不阻塞本次搜索。
/// 空查询默认列表只走前端 app 缓存、不调 search_apps，需要这个入口保证外部打开的
/// 应用/文件也能随获焦进入 TTL 刷新。
#[tauri::command]
pub async fn search_usage_tick() {
    usage::ensure_loaded().await;
    usage::maybe_refresh();
}

/// 返回全量应用元数据（不含图标），由前端做拼音匹配与排序。
/// 图标单独走 get_app_icons 批量拉取，避免 ~600KB base64 膨胀搜索热路径 IPC。
#[tauri::command]
pub async fn search_apps() -> Result<Vec<SearchResult>, String> {
    let (apps, _) = tokio::join!(get_cached_apps(), usage::ensure_loaded());
    usage::maybe_refresh();
    let index = usage::snapshot();

    let results: Vec<SearchResult> = apps
        .iter()
        .map(|app| {
            let effective = index.effective(&app.path);
            SearchResult {
                id: format!("app-{}", path_hash(&app.path)),
                title: app.name.clone(),
                path: app.path.clone(),
                kind: "application".to_string(),
                icon: None,
                last_used: effective.last_used_iso(),
                score: None,
                use_count: Some(effective.count),
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

/// 使用频率加权（log2 平滑，cap 1500），与前端 `frequencyBoost` 同源。
fn frequency_score(use_count: u32) -> i32 {
    if use_count == 0 {
        return 0;
    }
    let score = ((use_count as f64 + 1.0).ln() / std::f64::consts::LN_2 * 150.0).round() as i32;
    score.min(1500)
}

/// 最近使用加权（分桶），与前端 `recencyScore` 同源。
fn recency_score(last_used_hours: i64, now_hours: i64) -> i32 {
    if last_used_hours <= 0 {
        return 0;
    }
    let hours_ago = now_hours - last_used_hours;
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
}

/// 单条文件候选打分：先做名称/拼音匹配，命中后才查 usage（避免对全量未命中项做
/// 哈希查找）；返回 None = 名称与拼音均未命中。
fn candidate_score(
    f: &super::types::CachedFile,
    q_lower: &str,
    is_ascii: bool,
    now_hours: i64,
    effective_for: impl FnOnce(&str) -> usage::EffectiveUsage,
) -> Option<(i32, usage::EffectiveUsage)> {
    // 名称 substring 匹配
    let name_score = f.name_lower.find(q_lower).map(|idx| {
        let base = if idx == 0 { 1000 } else { 600 };
        base - idx as i32 * 4
    });

    // 拼音匹配（ASCII 查询 + CJK 文件名）：首字母或全拼 substring 命中
    let pinyin_score = if is_ascii && !f.pinyin_key.is_empty() {
        f.pinyin_key.contains(q_lower).then_some(300)
    } else {
        None
    };

    let base = name_score.max(pinyin_score)?;
    let effective = effective_for(&f.path);
    let folder_bonus = if f.is_folder { 240 } else { 0 };
    let score = base
        + frequency_score(effective.count)
        + recency_score(effective.last_used_hours(), now_hours)
        + folder_bonus;
    Some((score, effective))
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

    let (cache, _) = tokio::join!(get_cached_files(), usage::ensure_loaded());
    usage::maybe_refresh();
    let index = usage::snapshot();

    let now_hours = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64 / 3600)
        .unwrap_or(0);

    // 复合打分：name substring(前缀 1000 / 包含 600) + pinyin(300) + frequency(log2, cap 1500)
    //          + recency(<1h=300 / <24h=200 / <168h=100 / <720h=50) + folder 优先 240
    // 每条命中从 usage 索引取有效次数/最近使用（Spotlight + 本地记录），保证常用项前置、
    // 刚打开的条目在下一次搜索立即升位。
    let mut candidates: Vec<(usize, i32, usage::EffectiveUsage)> = cache
        .iter()
        .enumerate()
        .filter_map(|(i, f)| {
            let (score, effective) = candidate_score(f, &q_lower, is_ascii, now_hours, |path| {
                index.effective(path)
            })?;
            Some((i, score, effective))
        })
        .collect();

    // 同分时用 usage / 更浅路径收束，避免依赖缓存扫描序的静态任意序。
    candidates.sort_unstable_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| b.2.count.cmp(&a.2.count))
            .then_with(|| b.2.last_used.cmp(&a.2.last_used))
            .then_with(|| cache[a.0].path.len().cmp(&cache[b.0].path.len()))
            .then_with(|| cache[a.0].path.cmp(&cache[b.0].path))
    });
    candidates.truncate(100);

    let results: Vec<SearchResult> = candidates
        .iter()
        .map(|(i, _, effective)| {
            let f = &cache[*i];
            let kind_str = if f.is_folder { "folder" } else { "file" };
            SearchResult {
                id: format!("{}-{}", kind_str, path_hash(&f.path)),
                title: f.name.clone(),
                path: f.path.clone(),
                kind: kind_str.to_string(),
                icon: None,
                last_used: effective.last_used_iso(),
                score: None,
                use_count: Some(effective.count),
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
        .status()
        .await
        .map_err(|e| format!("Failed to launch: {e}"))?;

    // 本地使用记录：打开成功即生效（搜索排序下轮查询读取），并落盘跨重启保留。
    usage::record(&path);
    // 通知前端：使用记录已变，下次获焦重跑时重新落规范序（App 元数据缓存同步失效）。
    // 先于落盘发出：前端只需内存中的新频次，不必等 IO。
    usage::emit_updated(path.ends_with(".app"));
    match tokio::task::spawn_blocking(usage::persist).await {
        Ok(Err(e)) => log::warn!("search usage: persist failed: {e}"),
        Err(e) => log::warn!("search usage: persist task failed: {e}"),
        _ => {}
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequency_score_matches_frontend_boost() {
        assert_eq!(frequency_score(0), 0);
        assert_eq!(frequency_score(1), 150);
        assert_eq!(frequency_score(10), 519);
        assert_eq!(frequency_score(100), 999);
        assert_eq!(frequency_score(1_000_000), 1500);
    }

    #[test]
    fn recency_score_matches_frontend_buckets() {
        let now = 10_000;
        assert_eq!(recency_score(0, now), 0);
        assert_eq!(recency_score(now, now), 300);
        assert_eq!(recency_score(now - 1, now), 200);
        assert_eq!(recency_score(now - 23, now), 200);
        assert_eq!(recency_score(now - 24, now), 100);
        assert_eq!(recency_score(now - 167, now), 100);
        assert_eq!(recency_score(now - 168, now), 50);
        assert_eq!(recency_score(now - 719, now), 50);
        assert_eq!(recency_score(now - 720, now), 0);
        // 未来时间（时钟回拨）按最近期处理
        assert_eq!(recency_score(now + 5, now), 300);
    }

    fn file(name: &str, is_folder: bool) -> super::super::types::CachedFile {
        super::super::types::CachedFile {
            name: name.to_string(),
            name_lower: name.to_lowercase(),
            pinyin_key: super::super::pinyin::pinyin_key(name),
            path: format!("/tmp/{name}"),
            parent: None,
            is_folder,
        }
    }

    #[test]
    fn frequent_contains_folder_outranks_cold_prefix_file() {
        let now = 10_000;
        let common = usage::EffectiveUsage {
            count: 184,
            last_used: now * 3600,
        };
        let (folder_score, _) =
            candidate_score(&file("我的项目", true), "项目", false, now, |_| {
                common
            })
            .unwrap();
        let (cold_file_score, _) =
            candidate_score(&file("项目计划", false), "项目", false, now, |_| {
                usage::EffectiveUsage::default()
            })
            .unwrap();
        assert!(
            folder_score > cold_file_score,
            "常用 folder {folder_score} 应高于冷门 prefix file {cold_file_score}"
        );
    }

    #[test]
    fn candidate_score_returns_none_without_match() {
        let f = file("设计文档", false);
        assert_eq!(
            candidate_score(&f, "zzz", true, 0, |_| usage::EffectiveUsage::default()),
            None
        );
        assert!(candidate_score(&f, "sj", true, 0, |_| usage::EffectiveUsage::default()).is_some());
    }
}
