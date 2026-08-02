use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::AtomicU32;
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
    /// 累计使用次数（系统 use_count + 会话内增量），供前端打分加权。
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

/// 内存文件索引条目：启动时扫描目标子目录构建，search_files 走内存 substring 匹配（~3ms），
/// 不再 per-query spawn mdfind。name_lower 预计算消除热路径 toLowerCase 分配。
/// pinyin_key 预计算 CJK 文件名的拼音索引（首字母+全拼），让拼音查询（如 "sjwd"→"设计文档"）在 Rust 端即可召回。
/// use_count / last_used / last_used_hours 由一次 mdfind 批量拉取合并，
/// 让 Rust 排序时高频/近期文件前置（前端再做 fuzzy + boost 精排）。
#[derive(Debug, Clone)]
pub(super) struct CachedFile {
    pub name: String,
    pub name_lower: String,
    /// CJK 文件名拼音键（"首字母 全拼"），非 CJK 为空串。
    pub pinyin_key: String,
    pub path: String,
    pub parent: Option<String>,
    pub is_folder: bool,
    pub use_count: u32,
    /// Spotlight 格式原始字符串（透传给前端 recencyScore），None = 未被打开过。
    pub last_used: Option<String>,
    /// last_used 解析为 epoch hours（搜索时算 hours_ago 用），0 = 无数据。
    pub last_used_hours: i64,
}

pub(super) struct SearchSession {
    pub session_use_deltas: Mutex<HashMap<String, u32>>,
}

impl SearchSession {
    pub fn new() -> Self {
        Self {
            session_use_deltas: Mutex::new(HashMap::new()),
        }
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
pub(super) static FILE_CACHE: LazyLock<RwLock<Option<Arc<Vec<CachedFile>>>>> =
    LazyLock::new(|| RwLock::new(None));
pub(super) static APP_HANDLE: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();
pub(super) static SEARCH_SESSION: LazyLock<SearchSession> = LazyLock::new(SearchSession::new);
