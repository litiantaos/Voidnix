use nucleo_matcher::{
    pattern::{CaseMatching, Normalization, Pattern},
    Matcher, Utf32Str,
};
use std::cell::RefCell;
use std::io::BufRead;
use std::path::Path;
use std::sync::atomic::Ordering;

use super::cache::get_cached_apps;
use super::pinyin::{to_pinyin_full, to_pinyin_initials, word_pinyin_overrides};
use super::types::{SearchResult, SEARCH_SESSION};

thread_local! {
    static MATCHER: RefCell<Matcher> = RefCell::new(Matcher::default());
}

#[tauri::command]
#[cfg_attr(feature = "specta", specta::specta)]
pub async fn search_apps(query: String) -> Result<Vec<SearchResult>, String> {
    log::info!("search_apps called with query: '{}'", query);
    let apps = get_cached_apps().await;

    let results: Vec<SearchResult> = if query.trim().is_empty() {
        let mut sorted_apps = apps.iter().collect::<Vec<_>>();
        sorted_apps.sort_by(|a, b| b.last_used.cmp(&a.last_used));

        sorted_apps
            .into_iter()
            .take(50)
            .enumerate()
            .map(|(i, app)| SearchResult {
                id: format!("app-{}", i),
                title: app.name.clone(),
                path: app.path.clone(),
                kind: "application".to_string(),
                icon: app.icon_cache.clone(),
                last_used: app.last_used.clone(),
                score: Some((1000 - i as i32).max(0)),
            })
            .collect()
    } else {
        let results = MATCHER.with(|matcher_cell| {
            let mut matcher = matcher_cell.borrow_mut();
            let pattern = Pattern::parse(&query, CaseMatching::Ignore, Normalization::Smart);
            let mut buf = Vec::new();

            let mut scored_apps: Vec<(u32, &_)> = apps
                .iter()
                .filter_map(|app| {
                    let words = word_pinyin_overrides(&app.name);
                    let combined = format!(
                        "{} {} {} {} {} {}",
                        app.name,
                        app.bundle_name,
                        app.pinyin_full,
                        app.pinyin_initials,
                        app.pinyin_compact,
                        words
                    );

                    let mut score = pattern
                        .score(Utf32Str::new(&combined, &mut buf), &mut matcher)
                        .unwrap_or(0);

                    if score > 0 {
                        let system_count = app.use_count.load(Ordering::Relaxed);
                        let session_count = SEARCH_SESSION
                            .session_use_deltas
                            .lock()
                            .ok()
                            .and_then(|d| d.get(&app.path).copied())
                            .unwrap_or(0);
                        let count = system_count + session_count;
                        let boost = std::cmp::min(count * 10, 800);
                        score += boost;
                        Some((score, app))
                    } else {
                        None
                    }
                })
                .collect();

            scored_apps.sort_by_key(|b| std::cmp::Reverse(b.0));

            scored_apps
                .into_iter()
                .take(30)
                .enumerate()
                .map(|(i, (score, app))| SearchResult {
                    id: format!("app-{}", i),
                    title: app.name.clone(),
                    path: app.path.clone(),
                    kind: "application".to_string(),
                    icon: app.icon_cache.clone(),
                    last_used: app.last_used.clone(),
                    score: Some((score + 500) as i32),
                })
                .collect()
        });
        results
    };

    Ok(results)
}

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

    let scored_files: Vec<(u32, u32, String, String)> = MATCHER.with(|matcher_cell| {
        let mut matcher = matcher_cell.borrow_mut();
        let pattern = Pattern::parse(&query, CaseMatching::Ignore, Normalization::Smart);
        let mut buf = Vec::new();

        entries
            .into_iter()
            .filter_map(|(path, use_count)| {
                let name = Path::new(&path)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Unknown")
                    .to_string();

                let name_pinyin = to_pinyin_full(&name);
                let name_initials = to_pinyin_initials(&name);

                let parent = Path::new(&path)
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                let parent_pinyin = to_pinyin_full(parent);

                let compact = name_pinyin
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect::<String>();
                let words = word_pinyin_overrides(&name);
                let combined = format!(
                    "{} {} {} {} {} {}",
                    name, name_pinyin, name_initials, parent_pinyin, compact, words
                );

                let mut score = pattern
                    .score(Utf32Str::new(&combined, &mut buf), &mut matcher)
                    .unwrap_or(0);

                if score > 0 {
                    let boost = std::cmp::min(use_count * 10, 800);
                    score += boost;
                    Some((score, use_count, name, path))
                } else {
                    None
                }
            })
            .collect()
    });

    if SEARCH_SESSION.get_current_id() != search_id {
        return Ok(vec![]);
    }

    let results = tokio::task::spawn_blocking(move || {
        let mut files_with_meta: Vec<(u32, bool, String, String)> =
            Vec::with_capacity(scored_files.len());

        for (score, _use_count, name, path) in scored_files {
            if SEARCH_SESSION.get_current_id() != search_id {
                return Vec::new();
            }
            let is_dir = std::fs::metadata(&path)
                .map(|m| m.is_dir())
                .unwrap_or(false);
            files_with_meta.push((score, is_dir, name, path));
        }

        files_with_meta.sort_by_key(|b| std::cmp::Reverse(b.0));

        files_with_meta
            .into_iter()
            .take(50)
            .enumerate()
            .map(|(i, (score, is_dir, name, path))| {
                let kind_str = if is_dir { "folder" } else { "file" };
                SearchResult {
                    id: format!("{}-{}", kind_str, i),
                    title: name,
                    path,
                    kind: kind_str.to_string(),
                    icon: None,
                    last_used: None,
                    score: Some(score as i32),
                }
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|e| format!("Failed to compute file metadata: {}", e))?;

    Ok(results)
}

#[tauri::command]
pub async fn get_recent_apps() -> Result<Vec<SearchResult>, String> {
    let apps = get_cached_apps().await;

    let results = tokio::task::spawn_blocking(move || {
        let mut res: Vec<SearchResult> = apps
            .iter()
            .map(|app| SearchResult {
                id: format!("recent-{}", app.name.to_lowercase().replace(' ', "-")),
                title: app.name.clone(),
                path: app.path.clone(),
                kind: "application".to_string(),
                icon: app.icon_cache.clone(),
                last_used: app.last_used.clone(),
                score: Some(100),
            })
            .collect();

        res.sort_by(|a, b| b.last_used.cmp(&a.last_used));

        res.into_iter().take(10).collect()
    })
    .await
    .map_err(|e| e.to_string())?;

    Ok(results)
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

#[tauri::command]
pub async fn reveal_in_finder(path: String) -> Result<(), String> {
    let p = Path::new(&path);
    if !p.is_absolute() {
        return Err(format!("Path is not absolute: {}", path));
    }
    if !p.exists() {
        return Err(format!("Path does not exist: {}", path));
    }

    tokio::process::Command::new("open")
        .arg("-R")
        .arg(&path)
        .spawn()
        .map_err(|e| format!("Failed to reveal: {}", e))?;
    Ok(())
}

#[tauri::command]
#[cfg_attr(feature = "specta", specta::specta)]
pub fn score_items(query: String, items: Vec<String>) -> Vec<u32> {
    if query.trim().is_empty() {
        return vec![0; items.len()];
    }

    MATCHER.with(|matcher_cell| {
        let mut matcher = matcher_cell.borrow_mut();
        let pattern = Pattern::parse(&query, CaseMatching::Ignore, Normalization::Smart);
        let mut buf = Vec::new();

        items
            .into_iter()
            .map(|item| {
                let pinyin_full = to_pinyin_full(&item);
                let pinyin_initials = to_pinyin_initials(&item);
                let compact = pinyin_full
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect::<String>();
                let combined = format!("{} {} {} {}", item, pinyin_full, pinyin_initials, compact);

                pattern
                    .score(Utf32Str::new(&combined, &mut buf), &mut matcher)
                    .unwrap_or(0)
            })
            .collect()
    })
}
