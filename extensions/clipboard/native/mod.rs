use crate::infra::db::Database;
use base64::Engine;
use nucleo_matcher::{
    pattern::{CaseMatching, Normalization, Pattern},
    Matcher,
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use tauri::Manager;

thread_local! {
    static MATCHER: RefCell<Matcher> = RefCell::new(Matcher::default());
}

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
}

#[tauri::command]
#[cfg_attr(feature = "specta", specta::specta)]
pub async fn get_clipboard_history(
    query: Option<String>,
    filter_favorite: Option<bool>,
    limit: Option<u32>,
    app: tauri::AppHandle,
) -> Result<Vec<ClipboardItem>, String> {
    let db = app.state::<Database>();
    let conn = db.conn.lock().unwrap();

    let mut sql = "SELECT id, content, content_type, source_app, created_at, is_favorite FROM clipboard_history".to_string();
    if let Some(true) = filter_favorite {
        sql.push_str(" WHERE is_favorite = 1");
    }
    sql.push_str(" ORDER BY created_at DESC");

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| e.to_string())?;

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
            })
        })
        .map_err(|e| e.to_string())?;

    let mut items: Vec<ClipboardItem> = rows.filter_map(|r| r.ok()).collect();

    if let Some(q) = query {
        if !q.is_empty() {
            MATCHER.with(|m| {
                let mut matcher = m.borrow_mut();
                let pattern = Pattern::parse(&q, CaseMatching::Ignore, Normalization::Smart);

                for item in items.iter_mut() {
                    let mut text_to_match = item.content.clone();
                    // 如果只希望搜索剪贴板的内容，而不希望搜索到应用名称（比如搜索 fa 把 Safari 复制的所有东西都搜出来）
                    // 则不要把 source_app 拼接到搜索文本中
                    if item.content_type == "image" {
                        text_to_match = "图片 image".to_string();
                    } else if item.content_type == "file" {
                        text_to_match = format!("文件 file {}", item.content);
                    }

                    let mut buf = Vec::new();
                    if let Some(score) = pattern.score(
                        nucleo_matcher::Utf32Str::new(&text_to_match, &mut buf),
                        &mut matcher,
                    ) {
                        item.score = score as i32;
                    } else {
                        item.score = -1; // no match
                    }
                }
            });
            items.retain(|i| i.score >= 0);
            items.sort_by(|a, b| b.score.cmp(&a.score));
        }
    }

    if let Some(l) = limit {
        items.truncate(l as usize);
    } else {
        items.truncate(100);
    }

    Ok(items)
}

#[tauri::command]
pub async fn clear_clipboard_history(app: tauri::AppHandle) -> Result<(), String> {
    let db = app.state::<Database>();
    let conn = db.conn.lock().unwrap();
    conn.execute("DELETE FROM clipboard_history WHERE is_favorite = 0", [])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn toggle_clipboard_favorite(id: String, app: tauri::AppHandle) -> Result<(), String> {
    let db = app.state::<Database>();
    let conn = db.conn.lock().unwrap();
    conn.execute(
        "UPDATE clipboard_history SET is_favorite = NOT is_favorite WHERE id = ?1",
        rusqlite::params![id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn paste_clipboard_item(id: String, app: tauri::AppHandle) -> Result<(), String> {
    let item = {
        let db = app.state::<Database>();
        let conn = db.conn.lock().unwrap();

        let mut stmt = conn
            .prepare("SELECT content, content_type FROM clipboard_history WHERE id = ?1")
            .map_err(|e| e.to_string())?;

        stmt.query_row(rusqlite::params![id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
            .ok()
    };

    let (content, content_type) = match item {
        Some((c, t)) => (c, t),
        None => return Err(format!("Clipboard item not found: {id}")),
    };

    {
        use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString, NSPasteboardTypeFileURL, NSPasteboardTypePNG};
        use objc2_foundation::{NSString, NSData};

        unsafe {
            let pb = NSPasteboard::generalPasteboard();
            pb.clearContents();

            if content_type == "text" {
                let ns_string = NSString::from_str(&content);
                pb.setString_forType(&ns_string, NSPasteboardTypeString);
            } else if content_type == "file" {
                let ns_string = NSString::from_str(&content);
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

    // Hide our UI AND yield focus
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    
    #[cfg(target_os = "macos")]
    let _ = app.hide(); 

    tokio::spawn(async move {
        // Wait for macOS to completely finish the focus transition
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        
        tokio::task::spawn_blocking(move || {
            // Check Accessibility Permissions
            #[link(name = "ApplicationServices", kind = "framework")]
            extern "C" {
                fn AXIsProcessTrusted() -> bool;
            }
            
            unsafe {
                if !AXIsProcessTrusted() {
                    log::warn!("Accessibility permissions not granted! CGEventPost will silently fail.");
                }
            }

            // First try CGEvent (works for most apps)
            #[link(name = "CoreGraphics", kind = "framework")]
            extern "C" {
                fn CGEventSourceCreate(stateID: i32) -> *mut std::ffi::c_void;
                fn CGEventCreateKeyboardEvent(source: *mut std::ffi::c_void, keycode: u16, keydown: bool) -> *mut std::ffi::c_void;
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
                if !source.is_null() {
                    let v_down = CGEventCreateKeyboardEvent(source, KEY_V, true);
                    CGEventSetFlags(v_down, K_CG_EVENT_FLAG_MASK_COMMAND);
                    CGEventPost(K_CG_HID_EVENT_TAP, v_down);
                    CFRelease(v_down);

                    std::thread::sleep(std::time::Duration::from_millis(20));

                    let v_up = CGEventCreateKeyboardEvent(source, KEY_V, false);
                    CGEventSetFlags(v_up, K_CG_EVENT_FLAG_MASK_COMMAND);
                    CGEventPost(K_CG_HID_EVENT_TAP, v_up);
                    CFRelease(v_up);

                    CFRelease(source);
                }
            }


        });
    });

    Ok(())
}


pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new("clipboard")
        .setup(|app, _api| {
            crate::macos::clipboard_monitor::start_monitor(app.clone());
            Ok(())
        })
        .build()
}
