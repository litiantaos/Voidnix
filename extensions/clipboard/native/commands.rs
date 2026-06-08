use super::db::Database;
use crate::core::shortcut::set_window_visible;
use crate::infra::pinyin;
use base64::Engine;
use serde::{Deserialize, Serialize};
use tauri::Manager;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ClipboardItem {
    pub id: String,
    pub content: String,
    pub content_type: String,
    pub source_app: String,
    pub created_at: String,
    pub is_favorite: bool,
    pub score: i32,
    pub file_size: Option<i32>,
    pub image_width: Option<i32>,
    pub image_height: Option<i32>,
}

#[tauri::command]
#[cfg_attr(feature = "specta", specta::specta)]
pub async fn get_clipboard_history(
    query: Option<String>,
    filter_favorite: Option<bool>,
    limit: Option<u32>,
    preview_only: Option<bool>,
    app: tauri::AppHandle,
) -> Result<Vec<ClipboardItem>, String> {
    let db = app.state::<Database>();
    let conn = db.conn();

    let mut sql = "SELECT id, content, content_type, source_app, created_at, is_favorite, file_size, image_width, image_height FROM clipboard_history".to_string();
    if let Some(true) = filter_favorite {
        sql.push_str(" WHERE is_favorite = 1");
    }
    sql.push_str(" ORDER BY created_at DESC");
    let effective_limit = limit.unwrap_or(100);
    sql.push_str(&format!(" LIMIT {}", effective_limit));

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(ClipboardItem {
                id: row.get(0)?,
                content: row.get(1)?,
                content_type: row.get(2)?,
                source_app: row.get(3)?,
                created_at: row.get(4)?,
                is_favorite: row.get(5)?,
                score: 0,
                file_size: row.get(6)?,
                image_width: row.get(7)?,
                image_height: row.get(8)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut items: Vec<ClipboardItem> = rows.filter_map(|r| r.ok()).collect();

    if preview_only.unwrap_or(false) {
        for item in items.iter_mut() {
            if item.content_type == "image" {
                item.content = String::new();
            } else if item.content_type == "text" {
                let len = item.content.chars().count();
                if len > 200 {
                    item.content = item.content.chars().take(200).collect();
                }
            }
        }
    }

    if let Some(q) = query {
        if !q.is_empty() {
            pinyin::MATCHER.with(|m| {
                let mut matcher = m.borrow_mut();
                let pattern = pinyin::match_query(&q);

                for item in items.iter_mut() {
                    let text_to_match = if item.content_type == "image" {
                        "图片 image".to_string()
                    } else if item.content_type == "file" {
                        format!("文件 file {}", item.content)
                    } else {
                        item.content.clone()
                    };

                    let mut buf = Vec::new();
                    let score =
                        pinyin::pinyin_score(&text_to_match, &pattern, &mut matcher, &mut buf);
                    item.score = if score > 0 { score as i32 } else { -1 };
                }
            });
            items.retain(|i| i.score >= 0);
            items.sort_by_key(|b| std::cmp::Reverse(b.score));
        }
    }

    if let Some(l) = limit {
        items.truncate(l as usize);
    }

    Ok(items)
}

#[tauri::command]
pub async fn clear_clipboard_history(app: tauri::AppHandle) -> Result<(), String> {
    let db = app.state::<Database>();
    let conn = db.conn();
    conn.execute("DELETE FROM clipboard_history WHERE is_favorite = 0", [])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[cfg_attr(feature = "specta", specta::specta)]
pub async fn delete_clipboard_items(
    ids: Vec<String>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    if ids.is_empty() {
        return Ok(());
    }
    let db = app.state::<Database>();
    let conn = db.conn();
    let placeholders: String = ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("?{}", i + 1))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "DELETE FROM clipboard_history WHERE id IN ({})",
        placeholders
    );
    let params: Vec<&dyn rusqlite::types::ToSql> = ids
        .iter()
        .map(|id| id as &dyn rusqlite::types::ToSql)
        .collect();
    conn.execute(&sql, params.as_slice())
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[cfg_attr(feature = "specta", specta::specta)]
pub async fn toggle_clipboard_favorite(id: String, app: tauri::AppHandle) -> Result<(), String> {
    let db = app.state::<Database>();
    let conn = db.conn();
    conn.execute(
        "UPDATE clipboard_history SET is_favorite = NOT is_favorite WHERE id = ?1",
        rusqlite::params![id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[cfg_attr(feature = "specta", specta::specta)]
pub async fn get_clipboard_image(
    id: String,
    app: tauri::AppHandle,
) -> Result<Option<String>, String> {
    let db = app.state::<Database>();
    let conn = db.conn();

    let mut stmt = conn
        .prepare("SELECT content, content_type FROM clipboard_history WHERE id = ?1")
        .map_err(|e| e.to_string())?;

    let result: Option<(String, String)> = stmt
        .query_row(rusqlite::params![id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .ok();

    match result {
        Some((content, content_type)) if content_type == "image" => Ok(Some(content)),
        _ => Ok(None),
    }
}

fn write_to_pasteboard(content: &str, content_type: &str) {
    use objc2_app_kit::{
        NSPasteboard, NSPasteboardTypeFileURL, NSPasteboardTypePNG, NSPasteboardTypeString,
    };
    use objc2_foundation::{NSData, NSString};

    unsafe {
        let pb = NSPasteboard::generalPasteboard();
        pb.clearContents();

        let marker = NSString::from_str("com.litiantao.voidnix.clipboard");
        pb.setString_forType(&NSString::from_str(""), &marker);

        if content_type == "text" {
            let ns_string = NSString::from_str(content);
            pb.setString_forType(&ns_string, NSPasteboardTypeString);
        } else if content_type == "file" {
            let ns_string = NSString::from_str(content);
            pb.setString_forType(&ns_string, NSPasteboardTypeFileURL);
        } else if content_type == "image" {
            if let Some(base64_str) = content.strip_prefix("data:image/png;base64,") {
                if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(base64_str) {
                    let ns_data = NSData::with_bytes(&decoded);
                    pb.setData_forType(Some(&ns_data), NSPasteboardTypePNG);
                }
            }
        }
    }
}

fn hide_and_paste(app: &tauri::AppHandle) {
    crate::core::window::hide_main(app);
    set_window_visible(false);
    std::thread::spawn(|| simulate_cmd_v());
}

#[tauri::command]
#[cfg_attr(feature = "specta", specta::specta)]
pub fn paste_clipboard_item(id: String, app: tauri::AppHandle) -> Result<(), String> {
    let item = {
        let db = app.state::<Database>();
        let conn = db.conn();

        let mut stmt = conn
            .prepare("SELECT content, content_type FROM clipboard_history WHERE id = ?1")
            .map_err(|e| e.to_string())?;

        stmt.query_row(rusqlite::params![id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .ok()
    };

    let (content, content_type) = match item {
        Some((c, t)) => (c, t),
        None => return Err(format!("Clipboard item not found: {id}")),
    };

    write_to_pasteboard(&content, &content_type);
    hide_and_paste(&app);

    Ok(())
}

#[tauri::command]
#[cfg_attr(feature = "specta", specta::specta)]
pub fn paste_clipboard_items(ids: Vec<String>, app: tauri::AppHandle) -> Result<(), String> {
    let items = {
        let db = app.state::<Database>();
        let conn = db.conn();
        let placeholders: String = ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT content, content_type FROM clipboard_history WHERE id IN ({}) ORDER BY created_at DESC",
            placeholders
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let params: Vec<&dyn rusqlite::types::ToSql> = ids
            .iter()
            .map(|id| id as &dyn rusqlite::types::ToSql)
            .collect();
        let rows = stmt
            .query_map(params.as_slice(), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect::<Vec<_>>()
    };

    if items.is_empty() {
        return Err("No items found".to_string());
    }

    // 全部为文本时拼接，否则只粘贴最后一项
    let all_text = items.iter().all(|(_, t)| t == "text");
    let (content, content_type) = if all_text {
        let merged: String = items
            .iter()
            .map(|(c, _)| c.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        (merged, "text".to_string())
    } else {
        items.into_iter().last().unwrap()
    };

    write_to_pasteboard(&content, &content_type);
    hide_and_paste(&app);

    Ok(())
}

fn simulate_cmd_v() {
    std::thread::sleep(std::time::Duration::from_millis(200));

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }

    unsafe {
        if !AXIsProcessTrusted() {
            log::warn!("Accessibility permissions not granted! CGEventPost will silently fail.");
        }
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventSourceCreate(stateID: i32) -> *mut std::ffi::c_void;
        fn CGEventCreateKeyboardEvent(
            source: *mut std::ffi::c_void,
            keycode: u16,
            keydown: bool,
        ) -> *mut std::ffi::c_void;
        fn CGEventSetFlags(event: *mut std::ffi::c_void, flags: u64);
        fn CGEventPost(tapLocation: u32, event: *mut std::ffi::c_void);
        fn CFRelease(cf: *mut std::ffi::c_void);
    }

    const K_CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE: i32 = 1;
    const K_CG_HID_EVENT_TAP: u32 = 0;
    const K_CG_EVENT_FLAG_MASK_COMMAND: u64 = 0x00100000;
    const KEY_V: u16 = 0x09;

    unsafe {
        let source = CGEventSourceCreate(K_CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE);
        if source.is_null() {
            return;
        }

        let v_down = CGEventCreateKeyboardEvent(source, KEY_V, true);
        if !v_down.is_null() {
            CGEventSetFlags(v_down, K_CG_EVENT_FLAG_MASK_COMMAND);
            CGEventPost(K_CG_HID_EVENT_TAP, v_down);
            CFRelease(v_down);
        }

        std::thread::sleep(std::time::Duration::from_millis(20));

        let v_up = CGEventCreateKeyboardEvent(source, KEY_V, false);
        if !v_up.is_null() {
            CGEventSetFlags(v_up, K_CG_EVENT_FLAG_MASK_COMMAND);
            CGEventPost(K_CG_HID_EVENT_TAP, v_up);
            CFRelease(v_up);
        }

        CFRelease(source);
    }
}
