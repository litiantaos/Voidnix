use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use rusqlite::Connection;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::RwLock;

use crate::db::{self, CommandStat, SeqStat};
use crate::protocol::{Envelope, FeedbackReq, PingResp, RecordReq, SuggestReq, SuggestResp};
use crate::recall;
use crate::scorer::{self, RankContext, RankedResult, RecallKind};

const SEQ_CACHE_CAP: usize = 500;
const PROJECT_CACHE_CAP: usize = 256;
const ALT_TAKE: usize = 4;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
enum SeqKey {
    Bigram(String),
    Trigram(String, String),
    Recovery(String),
}

struct LruCache<K, V> {
    map: HashMap<K, (V, u64)>,
    counter: u64,
    cap: usize,
}

impl<K: Eq + std::hash::Hash + Clone, V: Clone> LruCache<K, V> {
    fn new(cap: usize) -> Self {
        Self {
            map: HashMap::new(),
            counter: 0,
            cap,
        }
    }

    /// 查询并刷新访问顺序，实现真正的 LRU 语义。
    /// 调用方需持 `&mut`，因此 server.rs 内统一升级到 `blocking_write`。
    fn get(&mut self, key: &K) -> Option<&V> {
        let counter = &mut self.counter;
        self.map.get_mut(key).map(|(_, ord)| {
            *counter += 1;
            *ord = *counter;
        })?;
        self.map.get(key).map(|(v, _)| v)
    }

    fn insert(&mut self, key: K, value: V) {
        if !self.map.contains_key(&key) && self.map.len() >= self.cap {
            if let Some(oldest) = self
                .map
                .iter()
                .min_by_key(|(_, (_, ord))| *ord)
                .map(|(k, _)| k.clone())
            {
                self.map.remove(&oldest);
            }
        }
        self.counter += 1;
        self.map.insert(key, (value, self.counter));
    }

    fn remove(&mut self, key: &K) {
        self.map.remove(key);
    }
}

struct StateInner {
    stats: Vec<CommandStat>,
    initials_index: HashMap<String, Vec<usize>>,
    seq_cache: LruCache<SeqKey, HashMap<String, SeqStat>>,
    dir_counts: HashMap<String, HashMap<String, i64>>,
    project_counts: HashMap<String, HashMap<String, i64>>,
    project_root_cache: LruCache<String, String>,
}

pub(crate) struct State {
    db: Mutex<Connection>,
    inner: RwLock<StateInner>,
}

pub struct Server {
    state: Arc<State>,
}

impl Server {
    /// 一次性查询入口，用于 daemon 不可用时的降级路径。
    /// 会从 DB 重建完整状态，仅适合偶尔调用。
    pub fn query_once(db_path: &Path, req: &SuggestReq) -> SuggestResp {
        let server = match Server::new(db_path) {
            Ok(s) => s,
            Err(_) => return SuggestResp::default(),
        };
        Self::suggest(&server.state, req)
    }

    pub fn new(db_path: &std::path::Path) -> Result<Self, String> {
        let conn = db::init_db(db_path).map_err(|e| format!("init db: {}", e))?;

        if let Err(e) = db::cleanup_old_commands(&conn, 90) {
            eprintln!("cleanup warning: {}", e);
        }

        let stats = db::get_command_stats(&conn).map_err(|e| format!("get stats: {}", e))?;

        // 批量加载 dir_counts + project_counts，避免按 stat 逐条查询的 N+1。
        let dir_counts = db::get_all_dir_counts(&conn).unwrap_or_default();
        let project_counts = db::get_all_project_counts(&conn).unwrap_or_default();

        let initials_index = recall::build_initials_index(&stats);

        Ok(Server {
            state: Arc::new(State {
                db: Mutex::new(conn),
                inner: RwLock::new(StateInner {
                    stats,
                    initials_index,
                    seq_cache: LruCache::new(SEQ_CACHE_CAP),
                    dir_counts,
                    project_counts,
                    project_root_cache: LruCache::new(PROJECT_CACHE_CAP),
                }),
            }),
        })
    }

    pub async fn serve(self, socket_path: &str) -> Result<(), String> {
        if std::path::Path::new(socket_path).exists() {
            if !self::is_live_socket(socket_path).await {
                std::fs::remove_file(socket_path).ok();
            } else {
                return Err(format!("daemon already running at {}", socket_path));
            }
        }

        if let Some(parent) = std::path::Path::new(socket_path).parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let listener = UnixListener::bind(socket_path)
            .map_err(|e| format!("bind {}: {}", socket_path, e))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o600)).ok();
        }

        eprintln!("daemon: listening on {}", socket_path);

        loop {
            let (stream, _) = listener
                .accept()
                .await
                .map_err(|e| format!("accept: {}", e))?;

            let state = self.state.clone();
            tokio::spawn(async move {
                handle_connection(stream, state).await;
            });
        }
    }

    pub fn suggest(state: &State, req: &SuggestReq) -> SuggestResp {
        let project_root = resolve_project_root(state, &req.dir);

        let (bigram, trigram, recovery) = load_sequence_data(state, req);

        let inner = state.inner.blocking_read();

        let candidates = recall::recall(&inner.stats, &inner.initials_index, &req.buffer);

        if candidates.is_empty() {
            return SuggestResp::default();
        }

        let ctx = RankContext {
            dir: &req.dir,
            project_root: &project_root,
            prev: &req.prev,
            prev_prev: &req.prev_prev,
            prev_exit: req.prev_exit,
            bigram: &bigram,
            trigram: &trigram,
            recovery: &recovery,
            dir_counts: &inner.dir_counts,
            project_counts: &inner.project_counts,
        };

        let now = SystemTime::now();
        let ranked = scorer::rank(&candidates, &ctx, now);

        if ranked.is_empty() {
            return SuggestResp::default();
        }

        let alts: Vec<RankedResult> = scorer::diversify(&ranked, ALT_TAKE + 1);
        let suggestion = alts
            .first()
            .map(|r| r.command.clone())
            .unwrap_or_default();
        let kind = alts
            .first()
            .map(|r| kind_label(r.kind).to_string())
            .unwrap_or_default();
        let alternatives = alts.into_iter().skip(1).map(|r| r.command).collect();

        SuggestResp {
            suggestion,
            alternatives,
            kind,
        }
    }

    pub fn record(state: &State, req: &RecordReq) {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let project_root = resolve_project_root(state, &req.dir);

        let cmd = db::Command {
            command: req.command.clone(),
            directory: req.dir.clone(),
            timestamp: now,
            exit_code: req.exit_code,
            duration_ms: req.duration,
            session_id: req.session.clone(),
            project_root: project_root.clone(),
            prev_command: req.prev.clone(),
            prev_prev_command: req.prev_prev.clone(),
            prev_exit_code: req.prev_exit,
        };

        {
            let db = state.db.lock().unwrap();
            if let Err(e) = db::record_command(&db, &cmd) {
                eprintln!("daemon: record error: {}", e);
            }
        }

        let mut inner = state.inner.blocking_write();
        let key = req.command.trim().to_string();
        update_stats_after_record(&mut inner, &key, now, req.exit_code);

        // Refresh dir/project counts for this command
        let db = state.db.lock().unwrap();
        if let Ok(dc) = db::get_dir_counts_for_command(&db, &req.command) {
            inner.dir_counts.insert(req.command.clone(), dc);
        }
        if let Ok(pc) = db::get_project_counts_for_command(&db, &req.command) {
            if pc.is_empty() {
                inner.project_counts.remove(&req.command);
            } else {
                inner.project_counts.insert(req.command.clone(), pc);
            }
        }
        drop(db);

        // Invalidate sequence cache for keys whose totals changed
        invalidate_seq_cache(&mut inner, req);
    }

    pub fn feedback(state: &State, req: &FeedbackReq) {
        let kind = req.kind.as_str();
        if !matches!(kind, "accept" | "reject" | "impression") {
            return;
        }
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        {
            let db = state.db.lock().unwrap();
            if let Err(e) = db::record_feedback(&db, &req.command, kind, now) {
                eprintln!("daemon: feedback error: {}", e);
            }
        }

        let mut inner = state.inner.blocking_write();
        let key = req.command.trim();
        if key.is_empty() {
            return;
        }
        if let Some(stat) = inner.stats.iter_mut().find(|s| s.command == key) {
            match kind {
                "accept" => stat.accept_count += 1,
                "reject" => stat.reject_count += 1,
                "impression" => stat.suggested_count += 1,
                _ => {}
            }
        } else {
            let mut stat = CommandStat {
                command: key.to_string(),
                count: 0,
                last_used: now,
                fail_count: 0,
                accept_count: 0,
                reject_count: 0,
                suggested_count: 0,
            };
            match kind {
                "accept" => stat.accept_count = 1,
                "reject" => stat.reject_count = 1,
                "impression" => stat.suggested_count = 1,
                _ => {}
            }
            inner.stats.push(stat);
            let idx = inner.stats.len() - 1;
            for token in recall::initials_of(key) {
                inner.initials_index.entry(token).or_default().push(idx);
            }
        }
    }
}

fn load_sequence_data(
    state: &State,
    req: &SuggestReq,
) -> (
    HashMap<String, SeqStat>,
    HashMap<String, SeqStat>,
    HashMap<String, SeqStat>,
) {
    let mut bigram = HashMap::new();
    let mut trigram = HashMap::new();
    let mut recovery = HashMap::new();

    if req.prev.is_empty() {
        return (bigram, trigram, recovery);
    }

    // Bigram is always useful when prev exists.
    bigram = load_or_fetch(state, SeqKey::Bigram(req.prev.clone()), |db| {
        db::get_sequence_counts(db, &req.prev).unwrap_or_default()
    });

    if !req.prev_prev.is_empty() {
        let pp = req.prev_prev.clone();
        let p = req.prev.clone();
        trigram = load_or_fetch(state, SeqKey::Trigram(pp.clone(), p.clone()), |db| {
            db::get_trigram_counts(db, &pp, &p).unwrap_or_default()
        });
    }

    if req.prev_exit != 0 {
        recovery = load_or_fetch(state, SeqKey::Recovery(req.prev.clone()), |db| {
            db::get_recovery_counts(db, &req.prev).unwrap_or_default()
        });
    }

    (bigram, trigram, recovery)
}

fn load_or_fetch<F>(
    state: &State,
    key: SeqKey,
    fetch: F,
) -> HashMap<String, SeqStat>
where
    F: FnOnce(&Connection) -> HashMap<String, SeqStat>,
{
    // LRU 语义要求 get 持写锁。命中路径只持有 inner write lock 极短时间，
    // 不在锁内做 DB 查询；miss 路径先释放锁、再取 DB、再回写缓存。
    {
        let mut inner = state.inner.blocking_write();
        if let Some(cached) = inner.seq_cache.get(&key) {
            return cached.clone();
        }
    }
    let data = {
        let db = state.db.lock().unwrap();
        fetch(&db)
    };
    let mut inner = state.inner.blocking_write();
    inner.seq_cache.insert(key, data.clone());
    data
}

fn invalidate_seq_cache(inner: &mut StateInner, req: &RecordReq) {
    if !req.prev.is_empty() {
        inner.seq_cache.remove(&SeqKey::Bigram(req.prev.clone()));
        if !req.prev_prev.is_empty() {
            inner
                .seq_cache
                .remove(&SeqKey::Trigram(req.prev_prev.clone(), req.prev.clone()));
        }
        if req.prev_exit != 0 {
            inner.seq_cache.remove(&SeqKey::Recovery(req.prev.clone()));
        }
    }
}

fn update_stats_after_record(
    inner: &mut StateInner,
    key: &str,
    now: i64,
    exit_code: i32,
) {
    if key.is_empty() {
        return;
    }
    if let Some(stat) = inner.stats.iter_mut().find(|s| s.command == key) {
        stat.count += 1;
        stat.last_used = now;
        if exit_code != 0 {
            stat.fail_count += 1;
        }
        return;
    }

    inner.stats.push(CommandStat {
        command: key.to_string(),
        count: 1,
        last_used: now,
        fail_count: if exit_code != 0 { 1 } else { 0 },
        accept_count: 0,
        reject_count: 0,
        suggested_count: 0,
    });
    let i = inner.stats.len() - 1;
    for token in recall::initials_of(key) {
        inner.initials_index.entry(token).or_default().push(i);
    }
}

fn resolve_project_root(state: &State, dir: &str) -> String {
    if dir.is_empty() {
        return String::new();
    }
    let key = dir.to_string();
    {
        let mut inner = state.inner.blocking_write();
        if let Some(v) = inner.project_root_cache.get(&key) {
            return v.clone();
        }
    }
    let resolved = detect_project_root(Path::new(dir)).unwrap_or_default();
    let mut inner = state.inner.blocking_write();
    inner.project_root_cache.insert(key, resolved.clone());
    resolved
}

fn detect_project_root(start: &Path) -> Option<String> {
    const MARKERS: &[&str] = &[".git", "Cargo.toml", "package.json", "go.mod", "pyproject.toml"];
    let mut current: PathBuf = start.to_path_buf();
    loop {
        for marker in MARKERS {
            if current.join(marker).exists() {
                return Some(current.display().to_string());
            }
        }
        if !current.pop() {
            break;
        }
    }
    None
}

fn kind_label(kind: RecallKind) -> &'static str {
    match kind {
        RecallKind::Prefix => "prefix",
        RecallKind::Abbrev => "abbrev",
        RecallKind::Fuzzy => "fuzzy",
    }
}

async fn handle_connection(
    stream: tokio::net::UnixStream,
    state: Arc<State>,
) {
    let (reader, mut writer) = stream.into_split();
    let mut buf_reader = BufReader::new(reader);
    let mut line = String::new();

    if buf_reader.read_line(&mut line).await.is_err() {
        return;
    }

    let env: Envelope = match serde_json::from_str(&line) {
        Ok(e) => e,
        Err(_) => return,
    };

    match env.msg_type.as_str() {
        "suggest" => {
            if let Ok(req) = serde_json::from_value::<SuggestReq>(env.payload) {
                let state2 = state.clone();
                let resp = tokio::task::spawn_blocking(move || Server::suggest(&state2, &req))
                    .await
                    .unwrap_or_default();
                if let Ok(json) = serde_json::to_string(&resp) {
                    let _ = writer.write_all((json + "\n").as_bytes()).await;
                }
            }
        }
        "record" => {
            if let Ok(req) = serde_json::from_value::<RecordReq>(env.payload) {
                let state2 = state.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    Server::record(&state2, &req);
                })
                .await;
                let _ = writer.write_all(b"{\"ok\":true}\n").await;
            }
        }
        "feedback" => {
            if let Ok(req) = serde_json::from_value::<FeedbackReq>(env.payload) {
                let state2 = state.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    Server::feedback(&state2, &req);
                })
                .await;
                let _ = writer.write_all(b"{\"ok\":true}\n").await;
            }
        }
        "ping" => {
            let _ = writer.write_all(b"{\"pong\":true}\n").await;
        }
        _ => {}
    }
}

async fn is_live_socket(socket_path: &str) -> bool {
    if let Ok(stream) = tokio::net::UnixStream::connect(socket_path).await {
        let (reader, mut writer) = stream.into_split();
        let mut buf_reader = BufReader::new(reader);

        if writer
            .write_all(b"{\"type\":\"ping\",\"payload\":{}}\n")
            .await
            .is_err()
        {
            return false;
        }

        let mut line = String::new();
        if buf_reader.read_line(&mut line).await.is_err() {
            return false;
        }

        if let Ok(resp) = serde_json::from_str::<PingResp>(&line) {
            return resp.pong;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_project_root_finds_git_dir() {
        let dir = std::env::temp_dir().join(format!("vn-zsh-as-proj-{}", std::process::id()));
        let sub = dir.join("a/b/c");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        let root = detect_project_root(&sub).unwrap();
        assert_eq!(root, dir.display().to_string());
    }

    #[test]
    fn lru_get_updates_order() {
        let mut cache: LruCache<String, i32> = LruCache::new(2);
        cache.insert("a".to_string(), 1);
        cache.insert("b".to_string(), 2);

        // 读 a，使 b 成为最旧
        assert_eq!(cache.get(&"a".to_string()), Some(&1));
        cache.insert("c".to_string(), 3);

        // a 应仍在（被读过），b 应被淘汰
        assert!(cache.get(&"a".to_string()).is_some());
        assert!(cache.get(&"b".to_string()).is_none());
        assert!(cache.get(&"c".to_string()).is_some());
    }

    fn temp_db_path() -> std::path::PathBuf {
        static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "zsh-as-srv-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            n,
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("test.db")
    }

    #[test]
    fn server_record_then_suggest_roundtrip() {
        let path = temp_db_path();
        let server = Server::new(&path).unwrap();

        // 记录 3 条命令：git status（高频）、git commit、ls
        for _ in 0..5 {
            Server::record(
                &server.state,
                &RecordReq {
                    command: "git status".to_string(),
                    dir: "/tmp".to_string(),
                    exit_code: 0,
                    duration: 10,
                    session: "s1".to_string(),
                    prev: String::new(),
                    prev_prev: String::new(),
                    prev_exit: 0,
                },
            );
        }
        Server::record(
            &server.state,
            &RecordReq {
                command: "git commit".to_string(),
                dir: "/tmp".to_string(),
                exit_code: 0,
                duration: 10,
                session: "s1".to_string(),
                prev: "git status".to_string(),
                prev_prev: String::new(),
                prev_exit: 0,
            },
        );

        // 查询 "git "：应召回 git status / git commit
        let resp = Server::suggest(
            &server.state,
            &SuggestReq {
                buffer: "git ".to_string(),
                dir: "/tmp".to_string(),
                prev: String::new(),
                prev_prev: String::new(),
                prev_exit: 0,
            },
        );
        let all = std::iter::once(resp.suggestion.clone())
            .chain(resp.alternatives.iter().cloned())
            .collect::<Vec<_>>();
        assert!(all.contains(&"git status".to_string()));
        assert!(all.contains(&"git commit".to_string()));
    }

    #[test]
    fn server_feedback_does_not_create_suggestable_phantom() {
        let path = temp_db_path();
        let server = Server::new(&path).unwrap();

        // feedback 一条从未执行过的命令
        Server::feedback(
            &server.state,
            &FeedbackReq {
                command: "phantom cmd".to_string(),
                kind: "impression".to_string(),
                session: "s1".to_string(),
            },
        );

        // 查询空 buffer：phantom cmd 不应被建议
        let resp = Server::suggest(
            &server.state,
            &SuggestReq {
                buffer: String::new(),
                dir: "/tmp".to_string(),
                prev: String::new(),
                prev_prev: String::new(),
                prev_exit: 0,
            },
        );
        assert_ne!(resp.suggestion, "phantom cmd");
        assert!(!resp.alternatives.contains(&"phantom cmd".to_string()));
    }
}
