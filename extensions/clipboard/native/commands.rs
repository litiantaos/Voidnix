use super::db::Database;
use crate::runtime::shortcut::set_window_visible;
use base64::Engine;
use serde::{Deserialize, Serialize};
use tauri::Manager;

/// DB 未 manage（打开失败降级）时返回可读错误，避免 state 缺失 panic。
fn require_db(app: &tauri::AppHandle) -> Result<tauri::State<'_, Database>, String> {
    app.try_state::<Database>()
        .ok_or_else(|| "剪贴板数据库不可用".to_string())
}

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
    let db = require_db(&app)?;
    let conn = db.conn();

    let mut sql = "SELECT id, content, content_type, source_app, created_at, is_favorite, file_size, image_width, image_height FROM clipboard_history".to_string();
    if let Some(true) = filter_favorite {
        sql.push_str(" WHERE is_favorite = 1");
    }
    sql.push_str(" ORDER BY created_at DESC");
    let effective_limit = limit.unwrap_or(500);
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
    let db = require_db(&app)?;
    let conn = db.conn();
    conn.execute("DELETE FROM clipboard_history WHERE is_favorite = 0", [])
        .map_err(|e| e.to_string())?;
    db.maybe_checkpoint(&conn);
    Ok(())
}

#[tauri::command]
pub async fn delete_clipboard_items(ids: Vec<String>, app: tauri::AppHandle) -> Result<(), String> {
    if ids.is_empty() {
        return Ok(());
    }
    let db = require_db(&app)?;
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
    db.maybe_checkpoint(&conn);
    Ok(())
}

#[tauri::command]
pub async fn toggle_clipboard_favorite(id: String, app: tauri::AppHandle) -> Result<(), String> {
    let db = require_db(&app)?;
    let conn = db.conn();
    conn.execute(
        "UPDATE clipboard_history SET is_favorite = NOT is_favorite WHERE id = ?1",
        rusqlite::params![id],
    )
    .map_err(|e| e.to_string())?;
    db.maybe_checkpoint(&conn);
    Ok(())
}

#[tauri::command]
pub async fn get_clipboard_image(
    id: String,
    app: tauri::AppHandle,
) -> Result<Option<String>, String> {
    let db = require_db(&app)?;
    let conn = db.conn();

    let mut stmt = conn
        .prepare("SELECT content, content_type FROM clipboard_history WHERE id = ?1")
        .map_err(|e| e.to_string())?;

    let result: Option<(String, String)> = stmt
        .query_row(rusqlite::params![id], |row| Ok((row.get(0)?, row.get(1)?)))
        .ok();

    match result {
        Some((content, content_type)) if content_type == "image" => Ok(Some(content)),
        _ => Ok(None),
    }
}

/// 取文本类记录的完整内容（previewOnly 模式仅截 200 字，预览需全文）。
#[tauri::command]
pub async fn get_clipboard_text(
    id: String,
    app: tauri::AppHandle,
) -> Result<Option<String>, String> {
    let db = require_db(&app)?;
    let conn = db.conn();
    let mut stmt = conn
        .prepare("SELECT content, content_type FROM clipboard_history WHERE id = ?1")
        .map_err(|e| e.to_string())?;
    let result: Option<(String, String)> = stmt
        .query_row(rusqlite::params![id], |row| Ok((row.get(0)?, row.get(1)?)))
        .ok();
    match result {
        Some((content, content_type)) if content_type == "text" => Ok(Some(content)),
        _ => Ok(None),
    }
}

/// 编辑文本类记录内容（仅 text，非 text 拒绝）。
#[tauri::command]
pub async fn update_clipboard_text(
    id: String,
    content: String,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let db = require_db(&app)?;
    {
        let conn = db.conn();
        let updated = conn
            .execute(
                "UPDATE clipboard_history SET content = ?1 WHERE id = ?2 AND content_type = 'text'",
                rusqlite::params![content, id],
            )
            .map_err(|e| e.to_string())?;
        if updated == 0 {
            return Err("Clipboard item not found or not text".to_string());
        }
        db.maybe_checkpoint(&conn);
    }
    Ok(())
}

/// 写入系统剪贴板。图片路径先 decode/转 PNG 再 clear，避免失败时已清空却仍 Cmd+V。
fn write_to_pasteboard(content: &str, content_type: &str) -> Result<(), String> {
    use crate::platform::pasteboard;

    match content_type {
        "text" => {
            pasteboard::clear();
            pasteboard::set_custom("", "com.litiantao.voidnix.clipboard");
            pasteboard::set_string(content);
            Ok(())
        }
        "file" => {
            pasteboard::clear();
            pasteboard::set_custom("", "com.litiantao.voidnix.clipboard");
            pasteboard::set_file_url(content);
            Ok(())
        }
        "image" => {
            // 兼容任意 data:image/*;base64,<payload>；先 encode 再 clear+写板
            let idx = content
                .find(";base64,")
                .ok_or_else(|| "无效的图片数据".to_string())?;
            let payload = &content[idx + ";base64,".len()..];
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(payload)
                .map_err(|_| "图片 base64 解码失败".to_string())?;
            let png = pasteboard::encode_image_to_png(&decoded)
                .ok_or_else(|| "图片解码失败".to_string())?;
            pasteboard::clear();
            pasteboard::set_custom("", "com.litiantao.voidnix.clipboard");
            pasteboard::set_png_bytes(&png);
            Ok(())
        }
        other => Err(format!("不支持的剪贴板类型: {other}")),
    }
}

/// 多文件写入 pasteboard（每个 item 一个 file URL + 防回环 marker）。
fn write_files_to_pasteboard(urls: &[String]) {
    use crate::platform::pasteboard;
    pasteboard::clear();
    // writeObjects 会替换 items：marker 必须写进 item，不能先 set_custom 再 set_file_urls
    pasteboard::set_file_urls(urls, Some("com.litiantao.voidnix.clipboard"));
}

fn hide_and_paste(app: &tauri::AppHandle) {
    crate::runtime::window::hide_main(app);
    set_window_visible(false);
    std::thread::spawn(simulate_cmd_v);
}

#[tauri::command]
pub fn paste_clipboard_item(id: String, app: tauri::AppHandle) -> Result<(), String> {
    if !ax_trusted() {
        return Err("需授予辅助功能权限".to_string());
    }
    let item = {
        let db = require_db(&app)?;
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

    write_to_pasteboard(&content, &content_type)?;
    hide_and_paste(&app);

    Ok(())
}

#[tauri::command]
pub fn paste_clipboard_items(ids: Vec<String>, app: tauri::AppHandle) -> Result<(), String> {
    if !ax_trusted() {
        return Err("需授予辅助功能权限".to_string());
    }
    if ids.is_empty() {
        return Err("No items found".to_string());
    }
    // 按前端 ids 顺序取行（不用 created_at），保证多选粘贴序 = 选择序
    let items = {
        let db = require_db(&app)?;
        let conn = db.conn();
        let placeholders: String = ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT id, content, content_type FROM clipboard_history WHERE id IN ({})",
            placeholders
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let params: Vec<&dyn rusqlite::types::ToSql> = ids
            .iter()
            .map(|id| id as &dyn rusqlite::types::ToSql)
            .collect();
        let rows = stmt
            .query_map(params.as_slice(), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let by_id: std::collections::HashMap<String, (String, String)> = rows
            .filter_map(|r| r.ok())
            .map(|(id, c, t)| (id, (c, t)))
            .collect();
        ids.iter()
            .filter_map(|id| by_id.get(id).cloned())
            .collect::<Vec<_>>()
    };

    if items.is_empty() {
        return Err("No items found".to_string());
    }

    let all_text = items.iter().all(|(_, t)| t == "text");
    let all_file = items.iter().all(|(_, t)| t == "file");
    if all_text {
        let merged: String = items
            .iter()
            .map(|(c, _)| c.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        write_to_pasteboard(&merged, "text")?;
    } else if all_file {
        // 全 file：多 item pasteboard，对齐访达多选复制
        let urls: Vec<String> = items.into_iter().map(|(c, _)| c).collect();
        write_files_to_pasteboard(&urls);
    } else {
        // 混类型：只粘选择序首项（与 UI「主选」一致）
        let (content, content_type) = &items[0];
        write_to_pasteboard(content, content_type)?;
    }
    hide_and_paste(&app);

    Ok(())
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

/// 辅助功能权限检查（CGEventPost 注入需授权，否则静默失败）。
fn ax_trusted() -> bool {
    // SAFETY: AXIsProcessTrusted 是 Accessibility C API，无参数，仅查询当前进程可信状态
    unsafe { AXIsProcessTrusted() }
}

fn simulate_cmd_v() {
    std::thread::sleep(std::time::Duration::from_millis(200));
    crate::platform::input::post_combo("cmd+v", None);
}
