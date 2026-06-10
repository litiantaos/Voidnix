use std::fs;
use std::path::Path;

pub(super) fn get_app_metadata(app_path: &str) -> (String, Option<String>, u32) {
    let output = std::process::Command::new("mdls")
        .arg("-name")
        .arg("kMDItemDisplayName")
        .arg("-name")
        .arg("kMDItemLastUsedDate")
        .arg("-name")
        .arg("kMDItemUseCount")
        .arg(app_path)
        .output();

    let mut name = String::new();
    let mut last_used = None;
    let mut use_count = 0;

    if let Ok(out) = output {
        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines() {
            if line.starts_with("kMDItemDisplayName") {
                if let Some(val) = line.split('=').nth(1) {
                    let cleaned = val.trim().trim_matches('"').to_string();
                    name = if cleaned.ends_with(".app") {
                        cleaned
                            .strip_suffix(".app")
                            .unwrap_or(&cleaned)
                            .to_string()
                    } else {
                        cleaned
                    };
                }
            } else if line.starts_with("kMDItemLastUsedDate") {
                if let Some(val) = line.split('=').nth(1) {
                    let cleaned = val.trim().to_string();
                    if cleaned != "(null)" {
                        last_used = Some(cleaned);
                    }
                }
            } else if line.starts_with("kMDItemUseCount") {
                if let Some(val) = line.split('=').nth(1) {
                    if let Ok(count) = val.trim().parse::<u32>() {
                        use_count = count;
                    }
                }
            }
        }
    }

    if name.is_empty() || name == "(null)" {
        name = get_app_name_from_plist(app_path).unwrap_or_else(|| {
            std::path::Path::new(app_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Unknown")
                .to_string()
        });
    }

    (name, last_used, use_count)
}

fn get_app_name_from_plist(app_path: &str) -> Option<String> {
    let plist_path = Path::new(app_path)
        .join("Contents")
        .join("Info.plist");
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
            if path
                .extension()
                .is_some_and(|ext| ext == "app")
            {
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

pub(super) fn parse_mdfind_attr_output(
    stdout: &str,
) -> Vec<(String, String, Option<String>, u32)> {
    let mut results: Vec<(String, String, Option<String>, u32)> = Vec::new();
    let mut current_path: Option<String> = None;
    let mut current_name = String::new();
    let mut current_last_used: Option<String> = None;
    let mut current_use_count: u32 = 0;

    let mut flush =
        |path: &mut Option<String>,
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
                            cleaned
                                .strip_suffix(".app")
                                .unwrap_or(&cleaned)
                                .to_string()
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
                    cleaned
                        .strip_suffix(".app")
                        .unwrap_or(&cleaned)
                        .to_string()
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

pub(super) fn collect_apps_with_metadata() -> Vec<(String, String, Option<String>, u32)> {
    let search_dirs = [
        "/Applications",
        "/System/Applications",
        "/System/Library/CoreServices/Applications",
    ];

    let mut command = std::process::Command::new("mdfind");
    command.arg("kMDItemContentType == 'com.apple.application-bundle'");
    command.arg("-attr").arg("kMDItemDisplayName");
    command.arg("-attr").arg("kMDItemLastUsedDate");
    command.arg("-attr").arg("kMDItemUseCount");

    for dir in &search_dirs {
        command.arg("-onlyin").arg(dir);
    }

    if let Some(home_dir) = dirs::home_dir() {
        let home_apps = home_dir.join("Applications");
        if home_apps.exists() {
            command.arg("-onlyin").arg(&home_apps);
        }
    }

    let mut results = if let Ok(output) = command.output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_mdfind_attr_output(&stdout)
    } else {
        Vec::new()
    };

    let existing: std::collections::HashSet<String> =
        results.iter().map(|(p, _, _, _)| p.clone()).collect();

    let fs_scan_dirs: Vec<&str> = search_dirs.to_vec();
    let mut fs_apps: Vec<String> = Vec::new();
    for dir in &fs_scan_dirs {
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
            log::info!(
                "Found app via filesystem scan (not in mdfind): {}",
                path
            );
            let (name, last_used, use_count) = get_app_metadata(&path);
            results.push((path, name, last_used, use_count));
        }
    }

    let finder_path = "/System/Library/CoreServices/Finder.app".to_string();
    if Path::new(&finder_path).exists() && !existing.contains(&finder_path) {
        let (name, last_used, use_count) = get_app_metadata(&finder_path);
        results.push((finder_path, name, last_used, use_count));
    }

    results
}

