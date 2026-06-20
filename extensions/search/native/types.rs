use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::LazyLock;
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub path: String,
    pub kind: String,
    pub icon: Option<String>,
    pub last_used: Option<String>,
    pub score: Option<i32>,
    /// 累计使用次数（mdfind kMDItemUseCount + 会话内增量），供前端打分加权。
    pub use_count: Option<u32>,
    /// 文件父目录名（仅文件结果），供前端补充匹配字段。
    pub parent: Option<String>,
}

#[derive(Debug)]
pub(super) struct CachedApp {
    pub name: String,
    pub path: String,
    pub icon_cache: Option<String>,
    pub last_used: Option<String>,
    pub use_count: AtomicU32,
}

pub(super) struct SearchSession {
    pub search_id: AtomicU32,
    pub session_use_deltas: Mutex<HashMap<String, u32>>,
}

impl SearchSession {
    pub fn new() -> Self {
        Self {
            search_id: AtomicU32::new(0),
            session_use_deltas: Mutex::new(HashMap::new()),
        }
    }

    pub fn next_search_id(&self) -> u32 {
        self.search_id.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn get_current_id(&self) -> u32 {
        self.search_id.load(Ordering::SeqCst)
    }

    pub fn increment_use_count(&self, path: &str) {
        if let Ok(mut deltas) = self.session_use_deltas.lock() {
            *deltas.entry(path.to_string()).or_insert(0) += 1;
        }
    }

    pub fn take_deltas(&self) -> HashMap<String, u32> {
        self.session_use_deltas
            .lock()
            .map(|mut d| std::mem::take(&mut *d))
            .unwrap_or_default()
    }
}

pub(super) static APP_CACHE: LazyLock<RwLock<Option<Arc<Vec<CachedApp>>>>> =
    LazyLock::new(|| RwLock::new(None));
pub(super) static APP_HANDLE: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();
pub(super) static SEARCH_SESSION: LazyLock<SearchSession> = LazyLock::new(SearchSession::new);
