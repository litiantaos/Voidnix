use serde::{Deserialize, Serialize};
use std::sync::{Arc, LazyLock};
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
    /// 有效使用次数（Spotlight + 本地使用记录合并），供前端打分加权。
    pub use_count: Option<u32>,
    /// 文件父目录名（仅文件结果），供前端补充匹配字段。
    pub parent: Option<String>,
}

/// 应用索引条目：只存静态元数据；使用频次/近因由 usage 索引独立维护。
#[derive(Debug)]
pub(super) struct CachedApp {
    pub name: String,
    pub path: String,
    pub icon_cache: Option<String>,
    /// .app bundle 目录的 mtime，用于图标增量缓存：重建时 mtime 未变则复用旧图标，
    /// 跳过 NSWorkspace 提取（占图标提取 90%+ 耗时）。
    pub bundle_mtime: Option<std::time::SystemTime>,
}

/// 内存文件索引条目：启动时扫描目标子目录构建，search_files 走内存 substring 匹配（~3ms）。
/// name_lower 预计算消除热路径 toLowerCase 分配；pinyin_key 预计算 CJK 文件名拼音索引。
/// 使用频次/近因不在索引内——由独立 usage 索引按 TTL 刷新并与此处路径 join。
#[derive(Debug, Clone)]
pub(super) struct CachedFile {
    pub name: String,
    pub name_lower: String,
    /// CJK 文件名拼音键（"首字母串\x1f全拼串"，不可打印分隔符隔离两段），非 CJK 为空串。
    pub pinyin_key: String,
    pub path: String,
    pub parent: Option<String>,
    pub is_folder: bool,
}

pub(super) static APP_CACHE: LazyLock<RwLock<Option<Arc<Vec<CachedApp>>>>> =
    LazyLock::new(|| RwLock::new(None));
pub(super) static FILE_CACHE: LazyLock<RwLock<Option<Arc<Vec<CachedFile>>>>> =
    LazyLock::new(|| RwLock::new(None));
pub(super) static APP_HANDLE: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();
