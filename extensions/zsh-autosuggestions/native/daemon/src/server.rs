use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use rusqlite::Connection;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::RwLock;

use crate::db::{self, CommandStat};
use crate::protocol::{Envelope, PingResp, RecordReq, SuggestReq, SuggestResp};
use crate::scorer;

struct StateInner {
    stats: Vec<CommandStat>,
    seq_cache: HashMap<String, HashMap<String, i64>>,
    dir_counts: HashMap<String, HashMap<String, i64>>,
}

struct State {
    db: Mutex<Connection>,
    inner: RwLock<StateInner>,
}

pub struct Server {
    state: Arc<State>,
}

impl Server {
    pub fn new(db_path: &std::path::Path) -> Result<Self, String> {
        let conn = db::init_db(db_path).map_err(|e| format!("init db: {}", e))?;

        let stats = db::get_command_stats(&conn).map_err(|e| format!("get stats: {}", e))?;

        let mut dir_counts: HashMap<String, HashMap<String, i64>> = HashMap::new();
        for s in &stats {
            let dc = db::get_dir_counts_for_command(&conn, &s.command).unwrap_or_default();
            dir_counts.insert(s.command.clone(), dc);
        }

        Ok(Server {
            state: Arc::new(State {
                db: Mutex::new(conn),
                inner: RwLock::new(StateInner {
                    stats,
                    seq_cache: HashMap::new(),
                    dir_counts,
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

    fn suggest(state: &State, req: &SuggestReq) -> SuggestResp {
        let inner = state.inner.blocking_read();

        let seq_counts = {
            if !req.prev.is_empty() {
                if let Some(cached) = inner.seq_cache.get(&req.prev) {
                    cached.clone()
                } else {
                    drop(inner);
                    let db = state.db.lock().unwrap();
                    let mut inner = state.inner.blocking_write();
                    let counts =
                        db::get_sequence_counts(&db, &req.prev).unwrap_or_default();
                    inner.seq_cache.insert(req.prev.clone(), counts.clone());
                    counts
                }
            } else {
                HashMap::new()
            }
        };

        let inner = state.inner.blocking_read();
        let now = SystemTime::now();
        let ranked = scorer::rank(
            &inner.stats,
            &req.buffer,
            &req.dir,
            &req.prev,
            &seq_counts,
            &inner.dir_counts,
            now,
        );

        if ranked.is_empty() {
            return SuggestResp {
                suggestion: String::new(),
                alternatives: Vec::new(),
            };
        }

        let alts: Vec<String> = ranked
            .iter()
            .skip(1)
            .take(4)
            .map(|r| r.command.clone())
            .collect();

        SuggestResp {
            suggestion: ranked[0].command.clone(),
            alternatives: alts,
        }
    }

    fn record(state: &State, req: &RecordReq) {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let cmd = db::Command {
            command: req.command.clone(),
            directory: req.dir.clone(),
            timestamp: now,
            exit_code: req.exit_code,
            duration_ms: req.duration,
            session_id: req.session.clone(),
        };

        let db = state.db.lock().unwrap();
        if let Err(e) = db::record_command(&db, &cmd, &req.prev) {
            eprintln!("daemon: record error: {}", e);
        }

        let mut inner = state.inner.blocking_write();
        inner.stats = db::get_command_stats(&db).unwrap_or_else(|_| inner.stats.clone());

        if let Ok(dc) = db::get_dir_counts_for_command(&db, &req.command) {
            inner.dir_counts.insert(req.command.clone(), dc);
        }

        if !req.prev.is_empty() {
            inner.seq_cache.remove(&req.prev);
        }
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
                let resp = tokio::task::spawn_blocking(move || {
                    Server::suggest(&state2, &req)
                })
                .await
                .unwrap_or(SuggestResp {
                    suggestion: String::new(),
                    alternatives: Vec::new(),
                });
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
