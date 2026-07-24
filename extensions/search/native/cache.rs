use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use tauri::Emitter;
use tokio::sync::Mutex;

use super::app_discovery::collect_apps_with_metadata;
use super::icon::get_app_icon;
use super::types::{CachedApp, APP_CACHE, APP_HANDLE, SEARCH_SESSION};

static INIT_GUARD: std::sync::LazyLock<Mutex<()>> = std::sync::LazyLock::new(|| Mutex::new(()));

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

pub async fn prewarm_cache() {
    get_cached_apps().await;
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

pub fn init_app_watcher() {
    use notify::{recommended_watcher, RecursiveMode, Watcher};
    use std::time::Duration;
    use tokio::time::sleep;

    tauri::async_runtime::spawn(async {
        let (tx, mut rx) = tokio::sync::mpsc::channel(100);

        let watcher_res = recommended_watcher(move |res| {
            if let Ok(event) = res {
                let _ = tx.blocking_send(event);
            }
        });

        if let Ok(mut watcher) = watcher_res {
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

                    log::info!("Detected app folder changes, rebuilding cache...");

                    let new_cache = init_app_cache().await;
                    let mut cache_lock = APP_CACHE.write().await;
                    *cache_lock = Some(new_cache);
                    drop(cache_lock);
                    if let Some(app) = APP_HANDLE.get() {
                        let _ = app.emit("app-cache-updated", ());
                    }
                }
            }
        }
    });
}
