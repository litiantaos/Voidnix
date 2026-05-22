use rusqlite::Connection;
use std::sync::Mutex;
use std::path::PathBuf;
use std::fs;

pub struct Database {
    pub conn: Mutex<Connection>,
}

impl Database {
    pub fn new(db_path: PathBuf) -> Self {
        if let Some(parent) = db_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let conn = Connection::open(&db_path).expect("Failed to open database");
        
        // Initialize tables
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
        ).expect("Failed to create clipboard_history table");

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_clipboard_created_at ON clipboard_history(created_at)",
            [],
        ).expect("Failed to create clipboard_history index");
        
        Self {
            conn: Mutex::new(conn),
        }
    }
}
