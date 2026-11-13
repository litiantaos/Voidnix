use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use tauri::Emitter;
use tokio::sync::Mutex;

use super::app_discovery::{collect_apps_with_metadata, mdfind_app_count};
use super::icon::get_app_icon;
use super::pinyin;
use super::scan::{scan_dir_names, FILE_SCAN_TCC_DIRS};
use super::types::{CachedApp, CachedFile, APP_CACHE, APP_HANDLE, FILE_CACHE};
use super::usage;

static INIT_GUARD: std::sync::LazyLock<Mutex<()>> = std::sync::LazyLock::new(|| Mutex::new(()));
static FILE_INIT_GUARD: std::sync::LazyLock<Mutex<()>> =
    std::sync::LazyLock::new(|| Mutex::new(()));

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

    let (app_metas, mdfind_count) =
        match tokio::task::spawn_blocking(collect_apps_with_metadata).await {
            Ok(metas) => metas,
            Err(e) => {
                log::error!(
                    "collect_apps_with_metadata panicked or was cancelled: {:?}",
                    e
                );
                (Vec::new(), 0)
            }
        };

    // Spotlight 未就绪（开机后首次查询 mdfind 返回空，条目为 fs 兜底、含系统组件）：
    // 调度退避重试，mdfind 恢复后重建缓存恢复完整应用列表（否则兜底条目将伴随整个
    // 进程生命周期——fs watcher 只监听目录变更，不会自愈）。
    if mdfind_count == 0 {
        log::warn!("mdfind returned no apps (Spotlight not ready?); scheduling self-heal retry");
        schedule_mdfind_retry();
    }

    let mut apps = Vec::with_capacity(app_metas.len());

    for (path, name) in app_metas {
        let bundle_mtime = std::fs::metadata(&path)
            .ok()
            .and_then(|m| m.modified().ok());

        apps.push(CachedApp {
            name,
            path,
            icon_cache: None,
            bundle_mtime,
        });
    }

    let result = Arc::new(apps);
    log::info!(
        "App cache initialized with {} apps (icons loading in background)",
        result.len()
    );

    // 在调用方覆盖 APP_CACHE 之前读取旧图标缓存，供后台任务按 bundle mtime 增量复用。
    // 应用更新改图标的场景稀少，mtime 未变即跳过 NSWorkspace 重提取（占图标提取 90%+ 耗时）。
    let old_icons: HashMap<String, (Option<std::time::SystemTime>, String)> = {
        let cache = APP_CACHE.read().await;
        cache
            .as_ref()
            .map(|apps| {
                apps.iter()
                    .filter_map(|a| {
                        a.icon_cache
                            .clone()
                            .map(|icon| (a.path.clone(), (a.bundle_mtime, icon)))
                    })
                    .collect()
            })
            .unwrap_or_default()
    };

    let result_clone = result.clone();
    let app_count = result.len();
    tokio::spawn(async move {
        // 图标提取分块并行：CachedApp 含大字段，先展平为 plain struct 再分块，
        // 各块独立 spawn_blocking 并发提取（利用多核）。
        const CHUNK: usize = 20;

        #[derive(Clone)]
        struct AppForIcon {
            name: String,
            path: String,
            bundle_mtime: Option<std::time::SystemTime>,
            old_icon: Option<String>,
        }

        let plain: Vec<AppForIcon> = result_clone
            .iter()
            .map(|a| {
                let old_icon = old_icons.get(&a.path).and_then(|(old_mt, icon)| {
                    if a.bundle_mtime == *old_mt {
                        Some(icon.clone())
                    } else {
                        None
                    }
                });
                AppForIcon {
                    name: a.name.clone(),
                    path: a.path.clone(),
                    bundle_mtime: a.bundle_mtime,
                    old_icon,
                }
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
                            let icon = app.old_icon.or_else(|| get_app_icon(&app.path));
                            CachedApp {
                                name: app.name,
                                path: app.path,
                                icon_cache: icon,
                                bundle_mtime: app.bundle_mtime,
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

/// 启动时递归扫描目标子目录构建文件索引。
/// usage 元数据已抽离到独立 usage 索引（见 usage.rs），文件索引只存静态元数据；
/// 两者在 `search_files` 查询时按 path join，互不阻塞索引重建。
pub(super) async fn init_file_cache() -> Arc<Vec<CachedFile>> {
    log::info!("Starting file cache initialization...");

    // 受 TCC 管辖目录仅在全磁盘权限就绪后扫描，避免首次触碰触发系统弹窗。
    let tcc_allowed = crate::platform::permission::check_full_disk_access();
    let entries = tokio::task::spawn_blocking(move || {
        let mut files = Vec::new();
        if let Some(home) = dirs::home_dir() {
            for dir_name in scan_dir_names(tcc_allowed) {
                let dir = home.join(dir_name);
                if dir.exists() {
                    scan_files_recursive(&dir, &mut files, 0);
                }
            }
        }
        files
    })
    .await
    .unwrap_or_default();

    log::info!("File cache initialized with {} entries", entries.len());
    Arc::new(entries)
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
        });

        if is_dir {
            scan_files_recursive(&path, files, depth + 1);
        }
    }
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
    let (_apps, _files, _usage) = tokio::join!(
        get_cached_apps(),
        get_cached_files(),
        usage::ensure_loaded()
    );
}

/// Spotlight 自愈重试单飞守卫：并发 init（首个 get_cached_apps 与 fs watcher 重建重叠）
/// 只允许一条重试链，防重复重建。
static MDFIND_RETRY_RUNNING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// mdfind 空结果自愈：每 20s 探测（最多 5 次），Spotlight 恢复后重建 app 缓存并
/// emit app-cache-updated 驱动前端刷新；usage 由独立索引的 TTL 刷新负责，图标由
/// init_app_cache 自身的后台任务随后补齐（app-icons-updated）。
fn schedule_mdfind_retry() {
    if MDFIND_RETRY_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        use tokio::time::sleep;
        for _ in 0..5 {
            sleep(std::time::Duration::from_secs(20)).await;
            let count = tokio::task::spawn_blocking(mdfind_app_count)
                .await
                .unwrap_or_default();
            if count == 0 {
                continue;
            }
            log::info!("Spotlight recovered ({} apps), rebuilding app cache", count);
            usage::refresh_async();
            let apps = init_app_cache().await;
            *APP_CACHE.write().await = Some(apps);
            if let Some(app) = APP_HANDLE.get() {
                let _ = app.emit("app-cache-updated", ());
            }
            break;
        }
        MDFIND_RETRY_RUNNING.store(false, Ordering::SeqCst);
    });
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
/// 变更经 5s 防抖 + 60s 最小重建间隔后重建 app 缓存（init_app_cache 内部增量提取图标）。
async fn app_dir_watcher() {
    use notify::{recommended_watcher, RecursiveMode, Watcher};
    use std::time::{Duration, Instant};
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

    const DEBOUNCE: Duration = Duration::from_secs(5);
    const MIN_INTERVAL: Duration = Duration::from_secs(60);
    let mut last_rebuild: Option<Instant> = None;

    loop {
        if rx.recv().await.is_some() {
            sleep(DEBOUNCE).await;
            while rx.try_recv().is_ok() {}

            if let Some(last) = last_rebuild {
                let elapsed = last.elapsed();
                if elapsed < MIN_INTERVAL {
                    sleep(MIN_INTERVAL - elapsed).await;
                    while rx.try_recv().is_ok() {}
                }
            }
            last_rebuild = Some(Instant::now());

            log::info!("App directory changes detected, rebuilding app cache...");
            usage::refresh_async();
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

/// 监听文件扫描目录（~/Projects、~/Code，及全磁盘权限就绪后的 ~/Desktop 等
/// TCC 管辖目录），变更经 5s 防抖 + 60s 最小重建间隔后重建文件索引。
/// 事件路径预过滤：变更全部落在 FILE_IGNORE_DIRS 内时跳过（消除 target/node_modules 写入空转重建）。
/// 管辖目录另有 60s 轮询全磁盘状态的自愈：授权后（无→有）补监听并立即重建一次。
async fn file_dir_watcher() {
    use notify::{recommended_watcher, RecursiveMode, Watcher};
    use std::time::{Duration, Instant};
    use tokio::time::sleep;

    let (tx, mut rx) = tokio::sync::mpsc::channel(200);
    let watcher_res = recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            // 预过滤：至少一条变更路径不在忽略目录内才入队，消除 cargo build / npm install
            // 在 target/node_modules 下的大量写入导致的空转重建（FSEvents 天然递归，NonRecursive 无效）
            let has_relevant = event.paths.iter().any(|p| {
                !p.components().any(|c| match c {
                    std::path::Component::Normal(name) => {
                        name.to_str().is_some_and(|n| FILE_IGNORE_DIRS.contains(&n))
                    }
                    _ => false,
                })
            });
            if has_relevant {
                let _ = tx.blocking_send(());
            }
        }
    });

    let Ok(mut watcher) = watcher_res else {
        return;
    };

    // 受 TCC 管辖目录仅在全磁盘权限就绪后监听（首次触碰触发系统「访问文件夹」弹窗，
    // 启动期零触碰；授权后经下方轮询自愈补齐）
    let home = dirs::home_dir();
    let mut tcc_watched = false;
    if let Some(home) = &home {
        let tcc_allowed = crate::platform::permission::check_full_disk_access();
        tcc_watched = tcc_allowed;
        for dir_name in scan_dir_names(tcc_allowed) {
            let path = home.join(dir_name);
            if path.exists() {
                let _ = watcher.watch(&path, RecursiveMode::NonRecursive);
            }
        }
    }

    const DEBOUNCE: Duration = Duration::from_secs(5);
    const MIN_INTERVAL: Duration = Duration::from_secs(60);
    const FDA_POLL: Duration = Duration::from_secs(60);
    let mut last_rebuild: Option<Instant> = None;
    // 绝对截止时间而非 select 内联 sleep：后者随每轮循环重建，事件分支耗时/抢跑会
    // 反复重置 60s 计时，持续文件事件下轮询分支被饿死（授权后永不自愈）
    let mut next_fda_poll = tokio::time::Instant::now() + FDA_POLL;

    loop {
        tokio::select! {
            // 全磁盘授权自愈：无→有时补监听受管辖目录并立即重建一次索引。
            // open 系统 TCC 数据库探测免弹窗、微秒级，60s 轮询开销可忽略。
            _ = tokio::time::sleep_until(next_fda_poll), if !tcc_watched => {
                next_fda_poll += FDA_POLL;
                if !crate::platform::permission::check_full_disk_access() {
                    continue;
                }
                tcc_watched = true;
                if let Some(home) = &home {
                    for dir_name in FILE_SCAN_TCC_DIRS {
                        let path = home.join(dir_name);
                        if path.exists() {
                            let _ = watcher.watch(&path, RecursiveMode::NonRecursive);
                        }
                    }
                }
                log::info!("Full disk access granted, rescanning protected dirs...");
                usage::refresh_async();
                let new_files = init_file_cache().await;
                {
                    let mut cache = FILE_CACHE.write().await;
                    *cache = Some(new_files);
                }
                // 与事件分支同源记账：紧随的文件事件受 MIN_INTERVAL 保护，防背靠背全量重建
                last_rebuild = Some(Instant::now());
            }
            maybe = rx.recv() => {
                if maybe.is_none() {
                    break;
                }
                sleep(DEBOUNCE).await;
                while rx.try_recv().is_ok() {}

                // 最小重建间隔：活跃开发下 cargo build / npm install 持续产生文件事件，
                // 60s 间隔将重建次数从每 7s 一次降至每 60s 一次（~85% 降幅），
                // 用户对文件搜索索引延迟不敏感。
                if let Some(last) = last_rebuild {
                    let elapsed = last.elapsed();
                    if elapsed < MIN_INTERVAL {
                        sleep(MIN_INTERVAL - elapsed).await;
                        while rx.try_recv().is_ok() {}
                    }
                }
                last_rebuild = Some(Instant::now());

                log::info!("File directory changes detected, rebuilding file cache...");
                let new_files = init_file_cache().await;
                {
                    let mut cache = FILE_CACHE.write().await;
                    *cache = Some(new_files);
                }
            }
        }
    }
}
