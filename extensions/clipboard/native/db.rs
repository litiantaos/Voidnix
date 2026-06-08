use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Manager;

/// 剪贴板历史 SQLite 路径：`<app_data>/data/clipboard.db`
pub fn clipboard_db_path(app: &tauri::AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("data");
    let _ = fs::create_dir_all(&dir);
    dir.join("clipboard.db")
}

pub struct Database {
    conn: Mutex<Connection>,
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
        }
    }

    pub fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }
}
