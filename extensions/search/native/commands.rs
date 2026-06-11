use std::io::BufRead;
use std::path::Path;
use std::sync::atomic::Ordering;

use super::cache::get_cached_apps;
use super::types::{SearchResult, SEARCH_SESSION};

/// 返回全量应用列表（带 use_count + last_used），由前端做拼音匹配与排序。
#[tauri::command]
#[cfg_attr(feature = "specta", specta::specta)]
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
        .enumerate()
        .map(|(i, app)| {
            let count = app.use_count.load(Ordering::Relaxed)
                + session_deltas.get(&app.path).copied().unwrap_or(0);
            SearchResult {
                id: format!("app-{}", i),
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

/// 用 mdfind 拉候选，返回 (path, use_count) 原始列表，由前端打分排序。
#[tauri::command]
#[cfg_attr(feature = "specta", specta::specta)]
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
        .map_err(|e| format!("mdfind spawn failed: {}", e))?;
    let stdout = child.stdout.take().ok_or("no stdout")?;

    const MAX_ENTRIES: usize = 300;

    let read_entries = tokio::task::spawn_blocking(move || {
        let reader = std::io::BufReader::new(stdout);
        let mut results: Vec<(String, u32)> = Vec::new();
        let mut current_path = String::new();
        let mut current_use_count: u32 = 0;
        let mut has_pending = false;

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };
            let trimmed = line.trim();

            if trimmed.is_empty() {
                if has_pending {
                    results.push((
                        std::mem::take(&mut current_path),
                        std::mem::take(&mut current_use_count),
                    ));
                    has_pending = false;
                    if results.len() >= MAX_ENTRIES {
                        break;
                    }
                }
                continue;
            }

            if trimmed.starts_with('/') {
                if has_pending {
                    results.push((
                        std::mem::take(&mut current_path),
                        std::mem::take(&mut current_use_count),
                    ));
                    has_pending = false;
                    if results.len() >= MAX_ENTRIES {
                        break;
                    }
                }

                let parts: Vec<&str> = trimmed.split("   ").collect();
                current_path = parts
                    .first()
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();

                for part in &parts[1..] {
                    if let Some(val) = part.trim().strip_prefix("kMDItemUseCount = ") {
                        current_use_count = val.trim().parse().unwrap_or(0);
                    }
                }
                has_pending = true;
            } else if let Some(val) = trimmed.strip_prefix("kMDItemUseCount = ") {
                current_use_count = val.trim().parse().unwrap_or(0);
            }
        }

        if has_pending {
            results.push((
                std::mem::take(&mut current_path),
                std::mem::take(&mut current_use_count),
            ));
        }
        results
    });

    let entries =
        match tokio::time::timeout(std::time::Duration::from_secs(4), read_entries).await {
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

    let results = tokio::task::spawn_blocking(move || {
        entries
            .into_iter()
            .enumerate()
            .filter_map(|(i, (path, use_count))| {
                if SEARCH_SESSION.get_current_id() != search_id {
                    return None;
                }
                let name = Path::new(&path)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Unknown")
                    .to_string();
                let parent = Path::new(&path)
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string());
                let is_dir = std::fs::metadata(&path).map(|m| m.is_dir()).unwrap_or(false);
                let kind_str = if is_dir { "folder" } else { "file" };
                Some(SearchResult {
                    id: format!("{}-{}", kind_str, i),
                    title: name,
                    path,
                    kind: kind_str.to_string(),
                    icon: None,
                    last_used: None,
                    score: None,
                    use_count: Some(use_count),
                    parent,
                })
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|e| format!("Failed to compute file metadata: {}", e))?;

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
        let _ = ws.selectFile_inFileViewerRootedAtPath(
            Some(&ns_path),
            &NSString::from_str(""),
        );
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
        .map_err(|e| format!("Failed to launch: {}", e))?;

    SEARCH_SESSION.increment_use_count(&path);

    Ok(())
}
