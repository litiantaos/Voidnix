use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use tauri::Emitter;
use tokio::sync::Mutex;

use super::app_discovery::collect_apps_with_metadata;
use super::icon::get_app_icon;
use super::pinyin;
use super::types::{CachedApp, CachedFile, APP_CACHE, APP_HANDLE, FILE_CACHE, SEARCH_SESSION};

static INIT_GUARD: std::sync::LazyLock<Mutex<()>> = std::sync::LazyLock::new(|| Mutex::new(()));
static FILE_INIT_GUARD: std::sync::LazyLock<Mutex<()>> =
    std::sync::LazyLock::new(|| Mutex::new(()));

/// 文件索引扫描目标子目录（家目录下）
const FILE_SCAN_DIRS: &[&str] = &[
    "Desktop",
    "Documents",
    "Downloads",
    "Pictures",
    "Music",
    "Movies",
    "Projects",
    "Code",
];

/// 递归扫描时跳过的目录名（依赖/构建产物/缓存，文件数巨大且无搜索价值）
const FILE_IGNORE_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    ".next",
    ".nuxt",
    "dist",
    "build",
    "out",
    "target",
    ".cache",
    "__pycache__",
    ".venv",
    "venv",
    ".npm",
    ".yarn",
    ".pnpm-store",
    ".terraform",
    "DerivedData",
    ".swiftpm",
    ".bundle",
];

const FILE_MAX_DEPTH: u32 = 6;
const FILE_MAX_ENTRIES: usize = 50_000;

pub(super) async fn init_app_cache() -> Arc<Vec<CachedApp>> {
    log::info!("Starting app cache initialization...");

    let app_metas = match tokio::task::spawn_blocking(collect_apps_with_metadata).await {
        Ok(metas) => metas,
        Err(e) => {
            log::error!(
                "collect_apps_with_metadata panicked or was cancelled: {:?}",
                e
            );
            Vec::new()
        }
    };

    let session_deltas = SEARCH_SESSION.take_deltas();

    let mut apps = Vec::with_capacity(app_metas.len());

    for (path, name, last_used, system_use_count) in app_metas {
        let delta = session_deltas.get(&path).copied().unwrap_or(0);

        apps.push(CachedApp {
            name,
            path,
            icon_cache: None,
            last_used,
            use_count: std::sync::atomic::AtomicU32::new(system_use_count + delta),
        });
    }

    let result = Arc::new(apps);
    log::info!(
        "App cache initialized with {} apps (icons loading in background)",
        result.len()
    );

    let result_clone = result.clone();
    let app_count = result.len();
    tokio::spawn(async move {
        // 图标提取分块并行：CachedApp 含 AtomicU32 无法 Clone，
        // 先展平为 plain struct 再分块，各块独立 spawn_blocking 并发提取（利用多核）。
        const CHUNK: usize = 20;

        #[derive(Clone)]
        struct AppForIcon {
            name: String,
            path: String,
            last_used: Option<String>,
            use_count: u32,
        }

        let plain: Vec<AppForIcon> = result_clone
            .iter()
            .map(|a| AppForIcon {
                name: a.name.clone(),
                path: a.path.clone(),
                last_used: a.last_used.clone(),
                use_count: a.use_count.load(Ordering::Relaxed),
            })
            .collect();

        let handles: Vec<_> = plain
            .chunks(CHUNK)
            .map(|chunk| {
                let chunk = chunk.to_vec();
                tokio::task::spawn_blocking(move || {
                    chunk
                        .into_iter()
                        .map(|app| {
                            let icon = get_app_icon(&app.path);
                            CachedApp {
                                name: app.name,
                                path: app.path,
                                icon_cache: icon,
                                last_used: app.last_used,
                                use_count: std::sync::atomic::AtomicU32::new(app.use_count),
                            }
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();

        // 按分块顺序汇合，保持原始 app 序
        let mut apps_with_icons = Vec::with_capacity(app_count);
        for h in handles {
            match h.await {
                Ok(chunk) => apps_with_icons.extend(chunk),
                Err(e) => log::error!("icon chunk panicked: {:?}", e),
            }
        }

        let mut cache = APP_CACHE.write().await;
        *cache = Some(Arc::new(apps_with_icons));
        log::info!("Background icon loading complete for {} apps", app_count);

        if let Some(app) = APP_HANDLE.get() {
            let _ = app.emit("app-icons-updated", ());
        }
    });

    result
}

pub fn set_app_handle(handle: tauri::AppHandle) {
    let _ = APP_HANDLE.set(handle);
}

// ── 文件索引 ──

/// 启动时扫描目标子目录构建文件索引。两步并行：
/// 1) spawn_blocking 递归遍历文件系统（全量文件名）
/// 2) spawn_blocking mdfind 拉 use_count>0 的文件元数据（path → use_count + last_used）
///
/// 合并后存入 FILE_CACHE。两步独立无依赖，tokio::join 并发执行。
pub(super) async fn init_file_cache() -> Arc<Vec<CachedFile>> {
    log::info!("Starting file cache initialization...");

    let (entries, usage_map) = tokio::join!(
        tokio::task::spawn_blocking(|| {
            let mut files = Vec::new();
            if let Some(home) = dirs::home_dir() {
                for dir_name in FILE_SCAN_DIRS {
                    let dir = home.join(dir_name);
                    if dir.exists() {
                        scan_files_recursive(&dir, &mut files, 0);
                    }
                }
            }
            files
        }),
        tokio::task::spawn_blocking(query_file_usage)
    );

    let mut entries = entries.unwrap_or_default();
    let usage_map = usage_map.unwrap_or_default();

    // 合并 mdfind 元数据：use_count + last_used + last_used_hours
    let merged = if !usage_map.is_empty() {
        for f in &mut entries {
            if let Some((use_count, last_used)) = usage_map.get(&f.path) {
                f.use_count = *use_count;
                f.last_used = last_used.clone();
                f.last_used_hours = last_used
                    .as_ref()
                    .and_then(|s| parse_epoch_hours(s))
                    .unwrap_or(0);
            }
        }
        entries
    } else {
        entries
    };

    log::info!("File cache initialized with {} entries", merged.len());
    Arc::new(merged)
}

/// 递归扫描目录，收集文件/文件夹到 files。跳过隐藏项 + FILE_IGNORE_DIRS，
/// 深度上限 FILE_MAX_DEPTH 防止符号链接循环，数量上限 FILE_MAX_ENTRIES 防止内存膨胀。
fn scan_files_recursive(dir: &Path, files: &mut Vec<CachedFile>, depth: u32) {
    if depth > FILE_MAX_DEPTH || files.len() >= FILE_MAX_ENTRIES {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        if files.len() >= FILE_MAX_ENTRIES {
            break;
        }
        let path = entry.path();
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        // 跳过隐藏文件/目录（覆盖 .git/.cache/.config 等）
        if name.starts_with('.') {
            continue;
        }
        let is_dir = path.is_dir();
        if is_dir && FILE_IGNORE_DIRS.contains(&name.as_str()) {
            continue;
        }

        let parent = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());

        let py_key = pinyin::pinyin_key(&name);
        files.push(CachedFile {
            name_lower: name.to_lowercase(),
            name,
            pinyin_key: py_key,
            path: path.to_string_lossy().into_owned(),
            parent,
            is_folder: is_dir,
            use_count: 0,
            last_used: None,
            last_used_hours: 0,
        });

        if is_dir {
            scan_files_recursive(&path, files, depth + 1);
        }
    }
}

/// 一次 mdfind 拉目标目录下 use_count>0 的文件元数据（path → (use_count, last_used)）。
/// 只返回被打开过的文件（远少于全量），解析 kMDItemUseCount + kMDItemLastUsedDate 属性。
/// 10s 超时：超时则 kill 子进程并返回空 map（不影响基础索引功能，仅缺 use_count/recency 加权）。
fn query_file_usage() -> HashMap<String, (u32, Option<String>)> {
    let mut command = std::process::Command::new("mdfind");
    command.arg("kMDItemUseCount > 0");
    command.arg("-attr").arg("kMDItemUseCount");
    command.arg("-attr").arg("kMDItemLastUsedDate");

    if let Some(home) = dirs::home_dir() {
        for dir in FILE_SCAN_DIRS {
            let path = home.join(dir);
            if path.exists() {
                command.arg("-onlyin").arg(&path);
            }
        }
    }

    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::null());

    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };
    let stdout = match child.stdout.take() {
        Some(s) => s,
        None => return HashMap::new(),
    };

    // 跨线程超时：读线程 + 计时线程，先就绪的胜出
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = std::io::read_to_string(stdout);
        let _ = tx.send(result);
    });

    match rx.recv_timeout(std::time::Duration::from_secs(10)) {
        Ok(Ok(output)) => {
            let _ = child.wait();
            parse_file_usage_output(&output)
        }
        _ => {
            let _ = child.kill();
            let _ = child.wait();
            HashMap::new()
        }
    }
}

/// 解析 mdfind -attr 输出为 path → (use_count, last_used) 映射。
/// 格式同 app_discovery：路径行 + 缩进属性行，空行分隔条目。
fn parse_file_usage_output(output: &str) -> HashMap<String, (u32, Option<String>)> {
    let mut map = HashMap::new();
    let mut current_path = String::new();
    let mut current_use_count: u32 = 0;
    let mut current_last_used: Option<String> = None;

    macro_rules! flush {
        () => {
            if !current_path.is_empty() {
                if current_use_count > 0 || current_last_used.is_some() {
                    map.insert(
                        std::mem::take(&mut current_path),
                        (current_use_count, current_last_used.take()),
                    );
                } else {
                    current_path.clear();
                }
            }
        };
    }

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            flush!();
            continue;
        }
        if trimmed.starts_with('/') {
            flush!();
            let parts: Vec<&str> = trimmed.split("   ").collect();
            current_path = parts
                .first()
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            for part in &parts[1..] {
                parse_usage_attr(part.trim(), &mut current_use_count, &mut current_last_used);
            }
        } else {
            parse_usage_attr(trimmed, &mut current_use_count, &mut current_last_used);
        }
    }
    flush!();
    map
}

fn parse_usage_attr(part: &str, use_count: &mut u32, last_used: &mut Option<String>) {
    if let Some(val) = part.strip_prefix("kMDItemUseCount = ") {
        *use_count = val.trim().parse().unwrap_or(0);
    } else if let Some(val) = part.strip_prefix("kMDItemLastUsedDate = ") {
        let cleaned = val.trim().trim_matches('"');
        if !cleaned.is_empty() && cleaned != "(null)" {
            *last_used = Some(cleaned.to_string());
        }
    }
}

/// 解析 Spotlight 日期字符串为 epoch hours（Howard Hinnant days-from-civil 算法）。
/// 输入格式 "2024-01-15 12:30:00 +0000"；精度到小时，用于 recency 分桶足够。
/// 零外部依赖，纯整数运算。
fn parse_epoch_hours(s: &str) -> Option<i64> {
    let s = s.trim_matches('"');
    let mut parts = s.split_whitespace();
    let date = parts.next()?;
    let time = parts.next()?;

    let mut dp = date.split('-');
    let y: i64 = dp.next()?.parse().ok()?;
    let m: i64 = dp.next()?.parse().ok()?;
    let d: i64 = dp.next()?.parse().ok()?;

    let h: i64 = time.split(':').next()?.parse().ok().unwrap_or(0);

    // days-from-civil（1970-01-01 = day 0）
    let y_adj = if m <= 2 { y - 1 } else { y };
    let era = if y_adj >= 0 { y_adj } else { y_adj - 399 } / 400;
    let yoe = y_adj - era * 400;
    let m_adj = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * m_adj + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;

    Some(days * 24 + h)
}

pub(super) async fn get_cached_files() -> Arc<Vec<CachedFile>> {
    {
        let cache = FILE_CACHE.read().await;
        if let Some(files) = &*cache {
            return files.clone();
        }
    }

    let _guard = FILE_INIT_GUARD.lock().await;
    {
        let cache = FILE_CACHE.read().await;
        if let Some(files) = &*cache {
            return files.clone();
        }
    }

    let files = init_file_cache().await;

    {
        let mut cache = FILE_CACHE.write().await;
        if cache.is_none() {
            *cache = Some(files.clone());
        }
    }

    files
}

pub async fn prewarm_cache() {
    let (_apps, _files) = tokio::join!(get_cached_apps(), get_cached_files());
}

pub(super) async fn get_cached_apps() -> Arc<Vec<CachedApp>> {
    {
        let cache = APP_CACHE.read().await;
        if let Some(apps) = &*cache {
            return apps.clone();
        }
    }

    let _guard = INIT_GUARD.lock().await;
    {
        let cache = APP_CACHE.read().await;
        if let Some(apps) = &*cache {
            return apps.clone();
        }
    }

    let apps = init_app_cache().await;

    {
        let mut cache = APP_CACHE.write().await;
        if cache.is_none() {
            *cache = Some(apps.clone());
        }
    }

    // metadata 就绪：通知前端刷新应用列表（图标随后单独补全）
    if let Some(app) = APP_HANDLE.get() {
        let _ = app.emit("app-cache-updated", ());
    }

    apps
}

/// 初始化文件系统监听器：应用目录与文件目录分离监听，各自独立重建。
/// macOS FSEvents 递归监听子目录树（NonRecursive 标记在 macOS 不生效）。
/// 分离原因：~/Downloads 等文件目录的频繁变更（iCloud 同步/浏览器下载）与应用列表无关，
/// 不应触发 app 缓存重建（含 2-5s 图标重提取），否则图标空窗期内 get_app_icons 返回全 null。
pub fn init_fs_watchers() {
    tauri::async_runtime::spawn(app_dir_watcher());
    tauri::async_runtime::spawn(file_dir_watcher());
}

/// 监听应用目录（/Applications、/System/Applications、~/Applications），
/// 变更经 5s 防抖后重建 app 缓存（init_app_cache 内部 spawn_blocking 提取图标）。
async fn app_dir_watcher() {
    use notify::{recommended_watcher, RecursiveMode, Watcher};
    use std::time::Duration;
    use tokio::time::sleep;

    let (tx, mut rx) = tokio::sync::mpsc::channel(50);
    let watcher_res = recommended_watcher(move |res: notify::Result<notify::Event>| {
        if res.is_ok() {
            let _ = tx.blocking_send(());
        }
    });

    let Ok(mut watcher) = watcher_res else {
        return;
    };
    let _ = watcher.watch(Path::new("/Applications"), RecursiveMode::NonRecursive);
    let _ = watcher.watch(
        Path::new("/System/Applications"),
        RecursiveMode::NonRecursive,
    );
    if let Some(home) = dirs::home_dir() {
        let _ = watcher.watch(&home.join("Applications"), RecursiveMode::NonRecursive);
    }

    loop {
        if rx.recv().await.is_some() {
            sleep(Duration::from_secs(5)).await;
            while rx.try_recv().is_ok() {}

            log::info!("App directory changes detected, rebuilding app cache...");
            let new_apps = init_app_cache().await;
            {
                let mut cache = APP_CACHE.write().await;
                *cache = Some(new_apps);
            }
            if let Some(app) = APP_HANDLE.get() {
                let _ = app.emit("app-cache-updated", ());
            }
        }
    }
}

/// 监听文件扫描目录（~/Desktop、~/Documents 等），变更经 5s 防抖后重建文件索引。
async fn file_dir_watcher() {
    use notify::{recommended_watcher, RecursiveMode, Watcher};
    use std::time::Duration;
    use tokio::time::sleep;

    let (tx, mut rx) = tokio::sync::mpsc::channel(200);
    let watcher_res = recommended_watcher(move |res: notify::Result<notify::Event>| {
        if res.is_ok() {
            let _ = tx.blocking_send(());
        }
    });

    let Ok(mut watcher) = watcher_res else {
        return;
    };
    if let Some(home) = dirs::home_dir() {
        for dir in FILE_SCAN_DIRS {
            let path = home.join(dir);
            if path.exists() {
                let _ = watcher.watch(&path, RecursiveMode::NonRecursive);
            }
        }
    }

    loop {
        if rx.recv().await.is_some() {
            sleep(Duration::from_secs(5)).await;
            while rx.try_recv().is_ok() {}

            log::info!("File directory changes detected, rebuilding file cache...");
            let new_files = init_file_cache().await;
            {
                let mut cache = FILE_CACHE.write().await;
                *cache = Some(new_files);
            }
        }
    }
}
