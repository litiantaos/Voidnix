use super::db::Database;
use crate::runtime::shortcut::set_window_visible;
use base64::Engine;
use serde::{Deserialize, Serialize};
use tauri::Manager;

#[derive(Debug, Clone, Serialize, Deserialize)]
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
pub async fn get_clipboard_history(
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
    use crate::platform::pasteboard;

    // 委托至 platform::pasteboard（不再直访 NSPasteboard，无 unsafe）
    pasteboard::clear();

    // marker：clipboard 自身写入标记，monitor 据此跳过记录
    pasteboard::set_custom("", "com.litiantao.voidnix.clipboard");

    match content_type {
        "text" => pasteboard::set_string(content),
        "file" => pasteboard::set_file_url(content),
        "image" => {
            if let Some(base64_str) = content.strip_prefix("data:image/png;base64,") {
                if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(base64_str) {
                    pasteboard::set_png(&decoded);
                }
            }
        }
        _ => {}
    }
}

fn hide_and_paste(app: &tauri::AppHandle) {
    crate::runtime::window::hide_main(app);
    set_window_visible(false);
    std::thread::spawn(simulate_cmd_v);
}

#[tauri::command]
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

    crate::platform::input::post_combo("cmd+v", None);
}
