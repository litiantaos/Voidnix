use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use tauri::AppHandle;

/// 剪贴板历史 SQLite 路径：`<app_data>/extensions/clipboard/clipboard.db`
/// 失败回退到 `./clipboard.db`（与历史行为一致，避免 setup panic）。
pub fn clipboard_db_path(app: &AppHandle) -> PathBuf {
    crate::runtime::storage::ext_data_dir(app, "clipboard")
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("clipboard.db")
}

/// WAL checkpoint 触发阈值：每 N 次 INSERT 后执行 PRAGMA wal_checkpoint(TRUNCATE)，
/// 防止 clipboard.db-wal 长期累积（DELETE 不会回收 WAL 文件大小）。
const WAL_CHECKPOINT_INTERVAL: u32 = 200;

pub struct Database {
    conn: Mutex<Connection>,
    write_count: AtomicU32,
}

impl Database {
    pub fn new(db_path: PathBuf) -> Self {
        let conn = Connection::open(&db_path).expect("Failed to open clipboard database");
        conn.pragma_update(None, "journal_mode", "WAL")
            .expect("Failed to set WAL mode");

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
        .expect("Failed to create clipboard_history table");

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_clipboard_created_at ON clipboard_history(created_at)",
            [],
        )
        .expect("Failed to create clipboard_history index");

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

        Self {
            conn: Mutex::new(conn),
            write_count: AtomicU32::new(0),
        }
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
