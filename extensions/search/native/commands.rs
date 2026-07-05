use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::BufRead;
use std::path::Path;
use std::sync::atomic::Ordering;

use serde::Serialize;

use super::cache::get_cached_apps;
use super::types::{SearchResult, SEARCH_SESSION};

/// 基于 path 的稳定 hash id（同 path 同 id，进程内确定性）。
/// 用于 dedupe key 与 Vue :key 稳定化——缓存重建/mdfind 重查时同 path 保持同 key，DOM 复用而非重建。
fn path_hash(path: &str) -> String {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

/// 返回全量应用列表（带 use_count + last_used），由前端做拼音匹配与排序。
#[tauri::command]
pub async fn search_apps() -> Result<Vec<SearchResult>, String> {
    let apps = get_cached_apps().await;
    let session_deltas = SEARCH_SESSION
        .session_use_deltas
        .lock()
        .ok()
        .map(|m| m.clone())
        .unwrap_or_default();

    let results: Vec<SearchResult> = apps
        .iter()
        .map(|app| {
            let count = app.use_count.load(Ordering::Relaxed)
                + session_deltas.get(&app.path).copied().unwrap_or(0);
            SearchResult {
                id: format!("app-{}", path_hash(&app.path)),
                title: app.name.clone(),
                path: app.path.clone(),
                kind: "application".to_string(),
                icon: app.icon_cache.clone(),
                last_used: app.last_used.clone(),
                score: None,
                use_count: Some(count),
                parent: None,
            }
        })
        .collect();

    Ok(results)
}

/// mdfind 返回的原始条目（路径 + 元数据），一次性解析完毕。
struct FileEntry {
    path: String,
    use_count: u32,
    is_folder: bool,
}

/// 用 mdfind 拉候选，通过 kMDItemContentType 判断文件/文件夹类型，
/// 返回带元数据的原始列表，由前端打分排序。
#[tauri::command]
pub async fn search_files(query: String) -> Result<Vec<SearchResult>, String> {
    if query.trim().is_empty() {
        return Ok(vec![]);
    }

    let search_id = SEARCH_SESSION.next_search_id();

    let target_dirs = vec![
        "Desktop".to_string(),
        "Documents".to_string(),
        "Downloads".to_string(),
        "Pictures".to_string(),
        "Music".to_string(),
        "Movies".to_string(),
        "Projects".to_string(),
        "Code".to_string(),
    ];

    let mut command = std::process::Command::new("mdfind");
    command.arg("-name").arg(&query);
    command.arg("-attr").arg("kMDItemContentType");
    command.arg("-attr").arg("kMDItemUseCount");

    if let Some(home_dir) = dirs::home_dir() {
        for dir in &target_dirs {
            let path = home_dir.join(dir);
            if path.exists() {
                command.arg("-onlyin").arg(path);
            }
        }
    }

    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::null());

    let mut child = command
        .spawn()
        .map_err(|e| format!("mdfind spawn failed: {e}"))?;
    let stdout = child.stdout.take().ok_or("no stdout")?;

    const MAX_ENTRIES: usize = 100;

    let read_entries = tokio::task::spawn_blocking(move || {
        let reader = std::io::BufReader::new(stdout);
        let mut entries: Vec<FileEntry> = Vec::new();
        let mut current_path = String::new();
        let mut current_use_count: u32 = 0;
        let mut current_is_folder = false;
        let mut has_pending = false;

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };
            let trimmed = line.trim();

            if trimmed.is_empty() {
                if has_pending {
                    entries.push(FileEntry {
                        path: std::mem::take(&mut current_path),
                        use_count: std::mem::take(&mut current_use_count),
                        is_folder: current_is_folder,
                    });
                    has_pending = false;
                    if entries.len() >= MAX_ENTRIES {
                        break;
                    }
                }
                continue;
            }

            if trimmed.starts_with('/') {
                if has_pending {
                    entries.push(FileEntry {
                        path: std::mem::take(&mut current_path),
                        use_count: std::mem::take(&mut current_use_count),
                        is_folder: current_is_folder,
                    });
                    has_pending = false;
                    if entries.len() >= MAX_ENTRIES {
                        break;
                    }
                }

                let parts: Vec<&str> = trimmed.split("   ").collect();
                current_path = parts
                    .first()
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
                current_use_count = 0;
                current_is_folder = false;

                for part in &parts[1..] {
                    let part = part.trim();
                    if part.starts_with("kMDItemContentType = ") {
                        current_is_folder = part.contains("public.folder");
                    } else if part.starts_with("kMDItemUseCount = ") {
                        if let Some(val) = part.strip_prefix("kMDItemUseCount = ") {
                            current_use_count = val.trim().parse().unwrap_or(0);
                        }
                    }
                }
                has_pending = true;
            } else if let Some(val) = trimmed.strip_prefix("kMDItemContentType = ") {
                current_is_folder = val.contains("public.folder");
            } else if let Some(val) = trimmed.strip_prefix("kMDItemUseCount = ") {
                current_use_count = val.trim().parse().unwrap_or(0);
            }
        }

        if has_pending {
            entries.push(FileEntry {
                path: std::mem::take(&mut current_path),
                use_count: std::mem::take(&mut current_use_count),
                is_folder: current_is_folder,
            });
        }
        entries
    });

    let entries = match tokio::time::timeout(std::time::Duration::from_secs(3), read_entries).await
    {
        Ok(Ok(entries)) => entries,
        _ => {
            let _ = child.kill();
            return Err("Search timed out".to_string());
        }
    };

    let _ = child.kill();

    if SEARCH_SESSION.get_current_id() != search_id {
        return Ok(vec![]);
    }

    // 不再需要第二次 spawn_blocking，所有元数据已在第一次遍历中获取
    let results: Vec<SearchResult> = entries
        .into_iter()
        .filter_map(|entry| {
            if SEARCH_SESSION.get_current_id() != search_id {
                return None;
            }
            let name = Path::new(&entry.path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("Unknown")
                .to_string();
            let parent = Path::new(&entry.path)
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                .map(|s| s.to_string());
            let kind_str = if entry.is_folder { "folder" } else { "file" };
            let id = format!("{}-{}", kind_str, path_hash(&entry.path));
            Some(SearchResult {
                id,
                title: name,
                path: entry.path,
                kind: kind_str.to_string(),
                icon: None,
                last_used: None,
                score: None,
                use_count: Some(entry.use_count),
                parent,
            })
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
