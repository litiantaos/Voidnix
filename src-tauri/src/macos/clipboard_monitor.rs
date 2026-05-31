use crate::infra::db::Database;
use base64::{engine::general_purpose::STANDARD as base64, Engine as _};
use std::ffi::c_void;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

struct ClipboardSnapshot {
    content: String,
    content_type: String,
    file_size: Option<i32>,
    image_width: Option<i32>,
    image_height: Option<i32>,
    source_app: String,
}

pub fn start_monitor(app_handle: AppHandle) {
    let snapshots: Arc<Mutex<Option<ClipboardSnapshot>>> = Arc::new(Mutex::new(None));

    std::thread::spawn(move || {
        let mut last_change_count: isize = 0;

        loop {
            std::thread::sleep(Duration::from_millis(500));

            let snap_clone = snapshots.clone();
            let _app_clone = app_handle.clone();

            let (tx, rx) = std::sync::mpsc::channel::<isize>();

            let _ = app_handle.run_on_main_thread(move || {
                use objc2::msg_send;
                use objc2_app_kit::{
                    NSPasteboard, NSPasteboardTypeFileURL, NSPasteboardTypePNG, NSPasteboardTypeString, NSWorkspace,
                };

                unsafe {
                    let pb = NSPasteboard::generalPasteboard();
                    let change_count = pb.changeCount();

                    let _ = tx.send(change_count);
                    if change_count == last_change_count {
                        return;
                    }

                    let mut content = String::new();
                    let mut content_type = String::new();
                    let mut file_size: Option<i32> = None;
                    let mut image_width: Option<i32> = None;
                    let mut image_height: Option<i32> = None;

                    if let Some(s) = pb.stringForType(NSPasteboardTypeFileURL) {
                        content = s.to_string();
                        content_type = "file".to_string();
                        let path = content.strip_prefix("file://").unwrap_or(&content);
                        let decoded_path = percent_decode(path);
                        if let Ok(meta) = std::fs::metadata(&decoded_path) {
                            file_size = Some(meta.len().min(i32::MAX as u64) as i32);
                        }
                    } else if let Some(s) = pb.stringForType(NSPasteboardTypeString) {
                        let text = s.to_string().trim().to_string();

                        if text.is_empty() || text.chars().count() <= 2 {
                            return;
                        }

                        let is_all_emoji = text.chars().all(|c| {
                            let cp = c as u32;
                            (0x1F300..=0x1FAFF).contains(&cp)
                                || (0x2600..=0x27BF).contains(&cp)
                                || (0xFE00..=0xFE0F).contains(&cp)
                        });

                        if is_all_emoji {
                            return;
                        }

                        content = text;
                        content_type = "text".to_string();
                    } else if let Some(d) = pb.dataForType(NSPasteboardTypePNG) {
                        let ptr: *const c_void = msg_send![&d, bytes];
                        let len: usize = msg_send![&d, length];
                        if len > 0 && !ptr.is_null() {
                            let slice = std::slice::from_raw_parts(ptr as *const u8, len);
                            file_size = Some((len as u64).min(i32::MAX as u64) as i32);
                            if len >= 24 && slice[0..4] == [0x89, 0x50, 0x4E, 0x47] {
                                image_width = Some(u32::from_be_bytes(slice[16..20].try_into().unwrap()) as i32);
                                image_height = Some(u32::from_be_bytes(slice[20..24].try_into().unwrap()) as i32);
                            }
                            content = format!("data:image/png;base64,{}", base64.encode(slice));
                            content_type = "image".to_string();
                        }
                    }

                    if content.is_empty() {
                        return;
                    }

                    let mut source_app = String::from("Unknown");
                    let ws = NSWorkspace::sharedWorkspace();
                    if let Some(app) = ws.frontmostApplication() {
                        if let Some(name) = app.localizedName() {
                            source_app = name.to_string();
                        }
                    }

                    *snap_clone.lock().unwrap() = Some(ClipboardSnapshot {
                        content,
                        content_type,
                        file_size,
                        image_width,
                        image_height,
                        source_app,
                    });
                }
            });

            let Ok(new_change_count) = rx.recv() else {
                continue;
            };

            if new_change_count == last_change_count {
                continue;
            }
            last_change_count = new_change_count;

            let snap = snapshots.lock().unwrap().take();
            let Some(snap) = snap else {
                continue;
            };

            let db = app_handle.state::<Database>();
            let conn = db.conn();

            let mut top_stmt = conn
                .prepare("SELECT content FROM clipboard_history ORDER BY created_at DESC LIMIT 1")
                .unwrap();
            let last_content: Option<String> = top_stmt
                .query_row([], |row| row.get(0))
                .ok();

            if let Some(last) = last_content {
                if last == snap.content {
                    continue;
                }
            }

            let mut stmt = conn
                .prepare("SELECT is_favorite FROM clipboard_history WHERE content = ?1")
                .unwrap();

            let existing_items: Vec<bool> = stmt
                .query_map(rusqlite::params![snap.content], |row| row.get(0))
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
                .unwrap_or_default();

            let is_favorite = existing_items.iter().any(|&fav| fav);

            if !existing_items.is_empty() {
                let _ = conn.execute(
                    "UPDATE clipboard_history SET created_at = datetime('now'), source_app = ?2, file_size = ?3, image_width = ?4, image_height = ?5 WHERE content = ?1",
                    rusqlite::params![snap.content, snap.source_app, snap.file_size, snap.image_width, snap.image_height],
                );
                if is_favorite {
                    let _ = conn.execute(
                        "UPDATE clipboard_history SET is_favorite = 1 WHERE content = ?1",
                        rusqlite::params![snap.content],
                    );
                }
            } else {
                let id = format!(
                    "{}",
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_millis()
                );

                let _ = conn.execute(
                    "INSERT INTO clipboard_history (id, content, content_type, source_app, is_favorite, file_size, image_width, image_height) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    rusqlite::params![id, snap.content, snap.content_type, snap.source_app, is_favorite, snap.file_size, snap.image_width, snap.image_height],
                );
            }

            use tauri_plugin_store::StoreExt;
            let store = app_handle.store(crate::infra::path::SETTINGS_STORE_PATH);
            let mut max_days: i32 = 30;

            if let Ok(s) = store {
                if let Some(clipboard) = s.get("clipboard") {
                    if let Some(n) = clipboard.get("maxDays").and_then(|v| v.as_i64()) {
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
    });
}

fn percent_decode(s: &str) -> String {
    let mut result = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(
                std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""),
                16,
            ) {
                result.push(byte);
                i += 3;
                continue;
            }
        }
        result.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&result).into_owned()
}
