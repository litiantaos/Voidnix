use crate::infra::db::Database;
use base64::{engine::general_purpose::STANDARD as base64, Engine as _};
use objc2::msg_send;
use objc2_app_kit::{
    NSPasteboard, NSPasteboardTypeFileURL, NSPasteboardTypePNG, NSPasteboardTypeString, NSWorkspace,
};
use std::ffi::c_void;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

pub fn start_monitor(app_handle: AppHandle) {
    std::thread::spawn(move || {
        let mut last_change_count = NSPasteboard::generalPasteboard().changeCount();
        loop {
            std::thread::sleep(Duration::from_millis(500));
            unsafe {
                let pb = NSPasteboard::generalPasteboard();
                let change_count = pb.changeCount();
                if change_count == last_change_count {
                    continue;
                }
                last_change_count = change_count;

                let mut content = String::new();
                let mut content_type = String::new();

                // Try File URL
                if let Some(s) = pb.stringForType(NSPasteboardTypeFileURL) {
                    content = s.to_string();
                    content_type = "file".to_string();
                } else if let Some(s) = pb.stringForType(NSPasteboardTypeString) {
                    // Try Text
                    let text = s.to_string().trim().to_string();
                    
                    // Ignore empty text, extremely short strings (<= 2 chars, typically single emojis/symbols),
                    // or strings that only contain emoji characters
                    if text.is_empty() || text.chars().count() <= 2 {
                        continue;
                    }

                    // A simple heuristic to exclude strings that are entirely emoji
                    // Emojis generally fall into certain high unicode blocks
                    let is_all_emoji = text.chars().all(|c| {
                        let cp = c as u32;
                        (cp >= 0x1F300 && cp <= 0x1FAFF) || // Misc Symbols and Pictographs, Emoticons, Transport, Symbols
                        (cp >= 0x2600 && cp <= 0x27BF) ||   // Misc Symbols, Dingbats
                        (cp >= 0xFE00 && cp <= 0xFE0F)      // Variation Selectors
                    });

                    if is_all_emoji {
                        continue;
                    }
                    
                    content = text;
                    content_type = "text".to_string();
                } else if let Some(d) = pb.dataForType(NSPasteboardTypePNG) {
                    // Try Image
                    let ptr: *const c_void = msg_send![&d, bytes];
                    let len: usize = msg_send![&d, length];
                    if len > 0 && !ptr.is_null() {
                        let slice = std::slice::from_raw_parts(ptr as *const u8, len);
                        content = format!("data:image/png;base64,{}", base64.encode(slice));
                        content_type = "image".to_string();
                    }
                }

                if content.is_empty() {
                    continue;
                }

                // Get source app
                let mut source_app = String::from("Unknown");
                let ws = NSWorkspace::sharedWorkspace();
                if let Some(app) = ws.frontmostApplication() {
                    if let Some(name) = app.localizedName() {
                        source_app = name.to_string();
                    }
                }

                let db = app_handle.state::<Database>();
                let conn = match db.conn.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };

                let mut top_stmt = conn
                    .prepare("SELECT content FROM clipboard_history ORDER BY created_at DESC LIMIT 1")
                    .unwrap();
                let last_content: Option<String> = top_stmt
                    .query_row([], |row| row.get(0))
                    .ok();

                if let Some(last) = last_content {
                    if last == content {
                        continue; // Skip duplicate if it's already the newest item
                    }
                }

                // Check if it exists deeper in the history to prevent duplicates
                let mut stmt = conn
                    .prepare("SELECT is_favorite FROM clipboard_history WHERE content = ?1")
                    .unwrap();
                
                let existing_items: Vec<bool> = stmt
                    .query_map(rusqlite::params![content], |row| row.get(0))
                    .map(|rows| rows.filter_map(|r| r.ok()).collect())
                    .unwrap_or_default();

                // If any of the duplicates is marked as favorite, preserve it
                let is_favorite = existing_items.iter().any(|&fav| fav);

                if !existing_items.is_empty() {
                    // Delete all existing duplicates so we can move it to the top
                    let _ = conn.execute(
                        "DELETE FROM clipboard_history WHERE content = ?1",
                        rusqlite::params![content],
                    );
                }

                // Generate ID
                let id = format!(
                    "{}",
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_millis()
                );

                // Insert into DB
                let _ = conn.execute(
                    "INSERT INTO clipboard_history (id, content, content_type, source_app, is_favorite) VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![id, content, content_type, source_app, is_favorite],
                );

                use tauri_plugin_store::StoreExt;
                let store = app_handle.store("settings.json");
                let mut max_days: i32 = 30;

                if let Ok(store) = store {
                    if let Some(v) = store.get("clipboardMaxDays") {
                        if let Some(n) = v.as_i64() {
                            max_days = n as i32;
                        }
                    }
                }

                const MAX_ROWS: i64 = 5000;

                if max_days > 0 {
                    let _ = conn.execute(
                        "DELETE FROM clipboard_history WHERE is_favorite = 0 AND created_at < datetime('now', ?1)",
                        rusqlite::params![format!("-{} days", max_days)],
                    );
                } else {
                    let count: i64 = conn
                        .query_row("SELECT COUNT(*) FROM clipboard_history", [], |row| row.get(0))
                        .unwrap_or(0);
                    if count > MAX_ROWS {
                        let _ = conn.execute(
                            "DELETE FROM clipboard_history WHERE is_favorite = 0 AND created_at < (SELECT MIN(created_at) FROM (SELECT created_at FROM clipboard_history ORDER BY created_at DESC LIMIT ?1))",
                            rusqlite::params![MAX_ROWS],
                        );
                    }
                }
            }
        }
    });
}
