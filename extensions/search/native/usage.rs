//! 搜索 usage 索引：路径 → 使用次数 / 最近使用时间。
//!
//! 两个来源，按「次数取 max、时间取较新」合并，避免同一次打开被两边重复计数：
//! - **Spotlight**（外部打开，含 Finder）：`kMDItemUseCount` + `kMDItemLastUsedDate`，
//!   按 TTL 异步刷新，不重建 50k 文件索引；
//! - **本地记录**（经 Voidnix 打开）：`launch_app` 写入并落盘 `usage.json`，跨重启保留，
//!   保证刚打开的条目下一次搜索立即可见地升位。
//!
//! 文件/应用索引只存静态元数据，查询时与此索引 join——usage 生命周期独立于索引重建，
//! 高频项不会因一次 mdfind 超时被清空。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{LazyLock, Mutex, OnceLock, RwLock};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use super::app_discovery::query_app_usage;
use super::scan::scan_dir_names;
use super::types::APP_HANDLE;

/// Spotlight usage 刷新间隔；搜索热路径只做一次 TTL 判断。
const REFRESH_TTL_SECS: i64 = 60;
/// 本地记录上限：超出后淘汰最久未用的路径，约束 JSON 体积与加载开销。
const MAX_ENTRIES: usize = 5000;

/// 单条 usage：次数 + 最近使用（epoch 秒，0 = 无数据）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct UsageEntry {
    pub count: u32,
    pub last_used: i64,
}

/// `search-usage-updated` 事件负载：前端据此决定是否失效应用 metadata 缓存。
#[derive(Debug, Clone, Serialize)]
struct SearchUsageUpdated {
    app: bool,
}

/// 合并后的有效 usage（查询结果）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct EffectiveUsage {
    pub count: u32,
    pub last_used: i64,
}

impl EffectiveUsage {
    pub(super) fn last_used_hours(self) -> i64 {
        self.last_used.div_euclid(3600)
    }

    /// 最近使用时间 → 前端 `recencyScore` 可直接解析的 ISO 字符串。
    pub(super) fn last_used_iso(self) -> Option<String> {
        (self.last_used > 0).then(|| format_iso(self.last_used))
    }
}

#[derive(Default)]
struct UsageIndex {
    /// Spotlight 文件/文件夹 usage（按文件扫描目录查询）。
    spot_files: HashMap<String, UsageEntry>,
    /// Spotlight 应用 usage（按应用目录查询）。
    spot_apps: HashMap<String, UsageEntry>,
    /// 经 Voidnix 打开的本地记录（持久化）。
    local: HashMap<String, UsageEntry>,
}

static USAGE: LazyLock<RwLock<UsageIndex>> = LazyLock::new(|| RwLock::new(UsageIndex::default()));
static USAGE_FILE: OnceLock<PathBuf> = OnceLock::new();
static PERSIST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
static REFRESHING: AtomicBool = AtomicBool::new(false);
static LAST_REFRESH: AtomicI64 = AtomicI64::new(0);
static REFRESH_SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static INIT: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();

fn read_index() -> std::sync::RwLockReadGuard<'static, UsageIndex> {
    USAGE.read().unwrap_or_else(|e| e.into_inner())
}

fn write_index() -> std::sync::RwLockWriteGuard<'static, UsageIndex> {
    USAGE.write().unwrap_or_else(|e| e.into_inner())
}

/// 启动时加载本地记录（Spotlight 部分由 `ensure_loaded` 异步填充）。
pub(super) fn init(app: &AppHandle) {
    let Ok(dir) = crate::runtime::storage::ext_data_dir(app, "search") else {
        log::warn!("search usage: data dir unavailable; local usage will not persist");
        return;
    };
    let path = dir.join("usage.json");
    let local = match std::fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<HashMap<String, UsageEntry>>(&text) {
            Ok(map) => map,
            Err(e) => {
                log::warn!("search usage: parse {} failed: {e}", path.display());
                HashMap::new()
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
        Err(e) => {
            log::warn!("search usage: read {} failed: {e}", path.display());
            HashMap::new()
        }
    };
    write_index().local = local;
    let _ = USAGE_FILE.set(path);
}

/// 记录一次从 Voidnix 打开（调用方在打开成功后调用）。
pub(super) fn record(path: &str) {
    let now = now_secs();
    let mut index = write_index();
    let entry = index.local.entry(path.to_string()).or_insert(UsageEntry {
        count: 0,
        last_used: now,
    });
    entry.count = entry.count.saturating_add(1);
    entry.last_used = entry.last_used.max(now);
    prune_local(&mut index.local);
}

/// 本地记录落盘，返回错误供调用方记日志（调用方 spawn_blocking 消费，避免阻塞运行时）。
pub(super) fn persist() -> Result<(), String> {
    let Some(path) = USAGE_FILE.get() else {
        return Ok(());
    };
    let _guard = crate::runtime::lock_or_recover(&PERSIST_LOCK);
    let local = read_index().local.clone();
    let text = serde_json::to_string(&local).map_err(|e| e.to_string())?;
    crate::runtime::storage::atomic_write(path, &text)
}

/// 查询快照：一次读锁覆盖整轮候选打分，避免每条命中重复 lock。
pub(super) struct UsageSnapshot {
    index: std::sync::RwLockReadGuard<'static, UsageIndex>,
}

impl UsageSnapshot {
    pub(super) fn effective(&self, path: &str) -> EffectiveUsage {
        merge(
            self.index.spot_files.get(path).copied(),
            self.index.spot_apps.get(path).copied(),
            self.index.local.get(path).copied(),
        )
    }
}

pub(super) fn snapshot() -> UsageSnapshot {
    UsageSnapshot {
        index: read_index(),
    }
}

/// 确保 Spotlight usage 已至少加载一次（首次搜索等待，后续零成本）。
pub(super) async fn ensure_loaded() {
    INIT.get_or_init(refresh_serialized).await;
}

/// 通知前端 usage 已变（打开动作 / TTL 刷新）；`app` 决定是否失效应用 metadata 缓存。
pub(super) fn emit_updated(app: bool) {
    if let Some(handle) = APP_HANDLE.get() {
        let _ = handle.emit("search-usage-updated", SearchUsageUpdated { app });
    }
}

/// 搜索热路径调用：TTL 到期则后台刷新，不阻塞当前查询。
pub(super) fn maybe_refresh() {
    let now = now_secs();
    if now - LAST_REFRESH.load(Ordering::Relaxed) < REFRESH_TTL_SECS {
        return;
    }
    refresh_async();
}

/// 立即后台刷新（忽略 TTL，单飞）：权限/应用目录变化后主动补齐 usage。
pub(super) fn refresh_async() {
    if REFRESHING.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let _guard = RefreshGuard;
        refresh_serialized().await;
    });
}

struct RefreshGuard;

impl Drop for RefreshGuard {
    fn drop(&mut self) {
        REFRESHING.store(false, Ordering::SeqCst);
    }
}

/// 串行化所有刷新入口（初始加载 / TTL / 权限变化），避免慢查询的旧快照覆盖新快照。
async fn refresh_serialized() {
    let _guard = REFRESH_SERIAL.lock().await;
    refresh_now().await;
}

/// 一次刷新同时拉文件与应用 usage；任一查询失败保留旧快照（不把临时故障当成零使用）。
async fn refresh_now() {
    let initial = LAST_REFRESH.load(Ordering::Relaxed) == 0;
    let tcc_allowed = crate::platform::permission::check_full_disk_access();
    let (files, apps) = tokio::join!(
        tokio::task::spawn_blocking(move || query_file_usage(tcc_allowed)),
        tokio::task::spawn_blocking(query_app_usage),
    );

    let files = files.ok().flatten().map(to_entries);
    let apps = apps.ok().flatten().map(to_entries);
    let mut changed_apps = false;
    let mut changed = false;
    if files.is_some() || apps.is_some() {
        let mut index = write_index();
        if let Some(files) = files {
            changed |= index.spot_files != files;
            index.spot_files = files;
        }
        if let Some(apps) = apps {
            changed_apps = index.spot_apps != apps;
            changed |= changed_apps;
            index.spot_apps = apps;
        }
    }
    LAST_REFRESH.store(now_secs(), Ordering::Relaxed);
    // 首次加载由调用方（search_apps/search_files）在拿到快照后生成结果，无需额外通知；
    // 之后的刷新才需要让前端失效缓存 / 重新 settle。
    if changed && !initial {
        emit_updated(changed_apps);
    }
}

fn to_entries(raw: HashMap<String, (u32, Option<String>)>) -> HashMap<String, UsageEntry> {
    raw.into_iter()
        .filter_map(|(path, (count, last_used))| {
            let last_used = last_used
                .as_deref()
                .and_then(parse_spotlight_hours)
                .map(|hours| hours * 3600)
                .unwrap_or(0);
            (count > 0 || last_used > 0).then_some((path, UsageEntry { count, last_used }))
        })
        .collect()
}

fn merge(
    spot_files: Option<UsageEntry>,
    spot_apps: Option<UsageEntry>,
    local: Option<UsageEntry>,
) -> EffectiveUsage {
    let mut out = EffectiveUsage::default();
    for entry in [spot_files, spot_apps, local].into_iter().flatten() {
        out.count = out.count.max(entry.count);
        out.last_used = out.last_used.max(entry.last_used);
    }
    out
}

fn prune_local(local: &mut HashMap<String, UsageEntry>) {
    if local.len() <= MAX_ENTRIES {
        return;
    }
    let mut oldest: Vec<(String, i64)> = local
        .iter()
        .map(|(path, entry)| (path.clone(), entry.last_used))
        .collect();
    oldest.sort_unstable_by_key(|(_, last_used)| *last_used);
    let drop = local.len() - MAX_ENTRIES;
    for (path, _) in oldest.into_iter().take(drop) {
        local.remove(&path);
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 一次 mdfind 拉文件扫描目录下被打开过的文件元数据（path → (use_count, last_used)）。
/// 返回 None = 子进程失败/超时，调用方保留旧快照。
fn query_file_usage(tcc_allowed: bool) -> Option<HashMap<String, (u32, Option<String>)>> {
    let mut command = std::process::Command::new("mdfind");
    command.arg("kMDItemUseCount > 0");
    command.arg("-attr").arg("kMDItemUseCount");
    command.arg("-attr").arg("kMDItemLastUsedDate");

    let mut dirs = Vec::new();
    if let Some(home) = dirs::home_dir() {
        for dir in scan_dir_names(tcc_allowed) {
            let path = home.join(dir);
            if path.exists() {
                dirs.push(path);
            }
        }
    }
    // 没有任何可扫描目录时不带 -onlyin 会退化成全盘 mdfind（文件索引本身为空），
    // 直接返回空 usage。
    if dirs.is_empty() {
        return Some(HashMap::new());
    }
    for dir in dirs {
        command.arg("-onlyin").arg(dir);
    }

    let output = super::mdfind::run(command, std::time::Duration::from_secs(10))?;
    Some(parse_file_usage_output(&output))
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
fn parse_spotlight_hours(s: &str) -> Option<i64> {
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

/// epoch 秒 → ISO 8601 UTC（前端 parseUtcMs 直接消费）。
fn format_iso(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = rem / 3_600;
    let minute = rem % 3_600 / 60;
    let second = rem % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// days-from-civil 逆变换（Howard Hinnant civil_from_days），返回 (year, month, day)。
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = year + if month <= 2 { 1 } else { 0 };
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(count: u32, last_used: i64) -> UsageEntry {
        UsageEntry { count, last_used }
    }

    #[test]
    fn iso_format_matches_epoch() {
        assert_eq!(format_iso(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_iso(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(format_iso(-1), "1969-12-31T23:59:59Z");
    }

    #[test]
    fn spotlight_hours_parse_matches_iso() {
        let secs: i64 = 1_700_000_000;
        assert_eq!(
            parse_spotlight_hours("2023-11-14 22:13:20 +0000"),
            Some(secs.div_euclid(3600))
        );
        assert_eq!(parse_spotlight_hours("(null)"), None);
    }

    #[test]
    fn merge_takes_max_count_and_last_used() {
        let merged = merge(
            Some(entry(5, 1_700_000_000)),
            None,
            Some(entry(10, 1_600_000_000)),
        );
        assert_eq!(merged.count, 10);
        assert_eq!(merged.last_used, 1_700_000_000);

        let merged = merge(
            Some(entry(12, 1_700_000_000)),
            None,
            Some(entry(10, 1_700_000_100)),
        );
        assert_eq!(merged.count, 12);
        assert_eq!(merged.last_used, 1_700_000_100);
    }

    #[test]
    fn merge_prefers_app_spot_entry_when_file_spot_absent() {
        let merged = merge(None, Some(entry(7, 1_700_000_000)), None);
        assert_eq!(merged.count, 7);
        assert_eq!(
            merged.last_used_iso().as_deref(),
            Some("2023-11-14T22:13:20Z")
        );
    }

    #[test]
    fn replace_spot_converts_spotlight_time_to_seconds() {
        let mut raw = HashMap::new();
        raw.insert(
            "/a".to_string(),
            (3, Some("2023-11-14 22:13:20 +0000".to_string())),
        );
        raw.insert("/b".to_string(), (0, None));
        let target = to_entries(raw);
        assert_eq!(target.get("/a").copied().map(|e| e.count), Some(3));
        assert_eq!(
            target.get("/a").copied().map(|e| e.last_used),
            Some(1_700_000_000 / 3600 * 3600)
        );
        assert!(!target.contains_key("/b"));
    }

    #[test]
    fn parse_file_usage_output_handles_inline_attrs() {
        let output = "/Users/me/Documents/项目   kMDItemUseCount = 184   kMDItemLastUsedDate = 2026-11-08 15:43:32 +0000\n\n";
        let map = parse_file_usage_output(output);
        assert_eq!(
            map.get("/Users/me/Documents/项目"),
            Some(&(184, Some("2026-11-08 15:43:32 +0000".to_string())))
        );
    }

    #[test]
    fn prune_local_keeps_newest_entries() {
        let mut local: HashMap<String, UsageEntry> = (0..(MAX_ENTRIES + 2))
            .map(|i| (format!("/p/{i}"), entry(1, i as i64)))
            .collect();
        prune_local(&mut local);
        assert_eq!(local.len(), MAX_ENTRIES);
        assert!(local.contains_key(&format!("/p/{}", MAX_ENTRIES + 1)));
        assert!(!local.contains_key("/p/0"));
        assert!(!local.contains_key("/p/1"));
    }
}
