use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use tauri::AppHandle;

/// 剪贴板历史 SQLite 路径：`<app_data>/extensions/clipboard/clipboard.db`
pub fn clipboard_db_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(crate::runtime::storage::ext_data_dir(app, "clipboard")
        .map_err(|e| e.to_string())?
        .join("clipboard.db"))
}

/// WAL checkpoint 触发阈值：每 N 次 INSERT 后执行 PRAGMA wal_checkpoint(TRUNCATE)，
/// 防止 clipboard.db-wal 长期累积（DELETE 不会回收 WAL 文件大小）。
const WAL_CHECKPOINT_INTERVAL: u32 = 200;

pub struct Database {
    conn: Mutex<Connection>,
    write_count: AtomicU32,
}

impl Database {
    /// 打开并初始化库。失败返回 Err（调用方降级：不 manage、不启 monitor，命令 try_state 报错）。
    /// 禁止 expect/panic——磁盘权限或损坏不得拖垮整 app。
    pub fn open(db_path: PathBuf) -> Result<Self, String> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("创建剪贴板目录失败: {e}"))?;
        }

        let conn = Connection::open(&db_path).map_err(|e| format!("打开剪贴板数据库失败: {e}"))?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| format!("设置 WAL 失败: {e}"))?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS clipboard_history (
                id TEXT PRIMARY KEY,
                content TEXT,
                content_type TEXT,
                source_app TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                is_favorite BOOLEAN DEFAULT 0
            )",
            [],
        )
        .map_err(|e| format!("创建 clipboard_history 表失败: {e}"))?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_clipboard_created_at ON clipboard_history(created_at)",
            [],
        )
        .map_err(|e| format!("创建索引失败: {e}"))?;

        // 迁移：追加 file_size / image_width / image_height 列（已存在则忽略）
        let _ = conn.execute(
            "ALTER TABLE clipboard_history ADD COLUMN file_size INTEGER",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE clipboard_history ADD COLUMN image_width INTEGER",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE clipboard_history ADD COLUMN image_height INTEGER",
            [],
        );

        Ok(Self {
            conn: Mutex::new(conn),
            write_count: AtomicU32::new(0),
        })
    }

    pub fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 写入后调用：累计计数，达阈值则触发 WAL checkpoint（TRUNCATE 模式回收文件大小）。
    /// 调用方持 guard（避免再次 lock 死锁），checkpoint 在持锁线程内执行。
    pub fn maybe_checkpoint(&self, conn: &Connection) {
        let n = self.write_count.fetch_add(1, Ordering::Relaxed) + 1;
        if n >= WAL_CHECKPOINT_INTERVAL {
            // 重置计数（即使 checkpoint 失败也接受，下次达阈值重试）
            self.write_count.store(0, Ordering::Relaxed);
            // TRUNCATE 模式：WAL 截断为 0 字节
            let _ = conn.pragma_update(None, "wal_checkpoint", "TRUNCATE");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("voidnix_clipboard_test_{name}_{nanos}.db"))
    }

    #[test]
    fn open_creates_schema_and_is_writable() {
        let path = temp_db_path("ok");
        let db = Database::open(path.clone()).expect("open");
        {
            let conn = db.conn();
            conn.execute(
                "INSERT INTO clipboard_history (id, content, content_type, source_app) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params!["1", "hello", "text", "Test"],
            )
            .unwrap();
            let n: i64 = conn
                .query_row("SELECT COUNT(*) FROM clipboard_history", [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 1);
        }
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }

    #[test]
    fn open_fails_on_unusable_path() {
        // 以文件为「目录」父路径 → create_dir_all 或 open 失败
        let blocker = temp_db_path("blocker");
        std::fs::write(&blocker, b"not-a-dir").unwrap();
        let bad = blocker.join("nested").join("clipboard.db");
        match Database::open(bad) {
            Ok(_) => panic!("expected open to fail on file-as-parent path"),
            Err(err) => assert!(!err.is_empty()),
        }
        let _ = std::fs::remove_file(&blocker);
    }
}
