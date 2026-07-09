use super::db::Database;
use base64::{engine::general_purpose::STANDARD as base64, Engine as _};
use rusqlite::Connection;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

struct ClipboardSnapshot {
    content: String,
    content_type: String,
    file_size: Option<i32>,
    image_width: Option<i32>,
    image_height: Option<i32>,
    source_app: String,
}

/// M-cb1：已知密码管理器源 app 名（不区分大小写匹配）。
/// 这些 app 复制密码时未必设置 org.nspasteboard.ConcealedType marker，
/// 故按源 app 名兜底过滤，避免明文密码入库。
const PASSWORD_MANAGER_APPS: &[&str] = &[
    "1password",
    "bitwarden",
    "keepassxc",
    "keepassx",
    "dashlane",
    "lastpass",
    "enpass",
    "keeper",
    "robiform",
    "nordpass",
];

/// 启发式判定：内容本身像 secret（独立长 token / password= 赋值等）。
/// 保守策略——只拦截明显的 secret 形态，避免误伤正常代码/长串。
fn looks_like_secret(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    // 明显赋值形式：password= / passwd= / pwd= / secret= / token= / api_key=
    let lower = trimmed.to_ascii_lowercase();
    for prefix in [
        "password:",
        "password=",
        "passwd:",
        "passwd=",
        "pwd:",
        "pwd=",
        "secret:",
        "secret=",
        "token:",
        "token=",
        "api_key:",
        "api_key=",
    ] {
        if lower.starts_with(prefix) && trimmed.len() > prefix.len() + 3 {
            return true;
        }
    }
    // 私钥 PEM 头
    if trimmed.contains("-----BEGIN ") && trimmed.contains("PRIVATE KEY-----") {
        return true;
    }
    false
}

fn is_password_manager(app_name: &str) -> bool {
    let lower = app_name.to_ascii_lowercase();
    PASSWORD_MANAGER_APPS
        .iter()
        .any(|&name| lower == name || lower.contains(name))
}

pub fn start_monitor(app_handle: AppHandle) {
    std::thread::spawn(move || {
        // None = 首次轮询基准未建立（只取当前 changeCount，不读取内容），
        // 避免启动时把当前剪贴板已有内容入库一条。
        let mut last_change_count: Option<isize> = None;

        loop {
            std::thread::sleep(Duration::from_millis(500));

            let (tx, rx) = std::sync::mpsc::channel::<(isize, Vec<ClipboardSnapshot>)>();

            let last_for_closure = last_change_count;
            let _ = app_handle.run_on_main_thread(move || {
                use crate::platform::pasteboard;

                let change_count = pasteboard::change_count();

                let snapshots = match last_for_closure {
                    // 首次轮询：只建立基准，不读取内容
                    None => Vec::new(),
                    Some(last) if change_count == last => Vec::new(),
                    Some(_) => collect_clipboard_snapshots(),
                };

                let _ = tx.send((change_count, snapshots));
            });

            let Ok((new_change_count, snaps)) = rx.recv() else {
                continue;
            };

            match last_change_count {
                // 首次：建立基准，跳过本次内容处理
                None => {
                    last_change_count = Some(new_change_count);
                    continue;
                }
                Some(last) if new_change_count == last => continue,
                Some(_) => {
                    last_change_count = Some(new_change_count);
                }
            }

            if snaps.is_empty() {
                continue;
            }

            let db = app_handle.state::<Database>();
            let conn = db.conn();

            // 单条且与最后一条相同 → 跳过（连续重复优化，避免每次都刷新 created_at）
            if snaps.len() == 1 {
                let last_content: Option<String> = conn
                    .prepare(
                        "SELECT content FROM clipboard_history ORDER BY created_at DESC LIMIT 1",
                    )
                    .ok()
                    .and_then(|mut stmt| stmt.query_row([], |row| row.get(0)).ok());
                if last_content == Some(snaps[0].content.clone()) {
                    continue;
                }
            }

            // 每条入库（多文件场景产生多条）；id 用毫秒 + 序号偏移避免 PRIMARY KEY 冲突
            let base = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis();
            for (i, snap) in snaps.iter().enumerate() {
                let id = format!("{}", base + i as u128);
                process_snapshot(&conn, snap, &id);
            }

            // 由前端 config.ts invoke 推入的扩展自管参数（替代 Rust 直读 config.json）
            let max_days: i32 = crate::extensions::clipboard::load_max_days();

            const MAX_ROWS: i64 = 5000;

            if max_days > 0 {
                let _ = conn.execute(
                    "DELETE FROM clipboard_history WHERE is_favorite = 0 AND created_at < datetime('now', 'localtime', ?1)",
                    rusqlite::params![format!("-{} days", max_days)],
                );
            } else {
                let count: i64 = conn
                    .query_row("SELECT COUNT(*) FROM clipboard_history", [], |row| {
                        row.get(0)
                    })
                    .unwrap_or(0);
                if count > MAX_ROWS {
                    let _ = conn.execute(
                        "DELETE FROM clipboard_history WHERE is_favorite = 0 AND created_at < (SELECT MIN(created_at) FROM (SELECT created_at FROM clipboard_history ORDER BY created_at DESC LIMIT ?1))",
                        rusqlite::params![MAX_ROWS],
                    );
                }
            }

            let _ = app_handle.emit("clipboard-updated", ());

            // 写入计数 + WAL checkpoint（防止 clipboard.db-wal 无限增长）
            db.maybe_checkpoint(&conn);
        }
    });
}

/// 从当前剪贴板收集所有 snapshot（多文件复制时返回多条）。须在主线程调用。
fn collect_clipboard_snapshots() -> Vec<ClipboardSnapshot> {
    use crate::platform::pasteboard;
    use objc2_app_kit::NSWorkspace;

    // 跳过自身防回环 marker + transient/concealed/auto_gen（密码管理器）
    if pasteboard::has_type("com.litiantao.voidnix.clipboard")
        || pasteboard::has_type("org.nspasteboard.TransientType")
        || pasteboard::has_type("org.nspasteboard.ConcealedType")
        || pasteboard::has_type("org.nspasteboard.AutoGeneratedType")
    {
        return Vec::new();
    }

    let mut snaps: Vec<ClipboardSnapshot> = Vec::new();

    // file URL 优先（多文件遍历 pasteboardItems）：访达复制文件常带图标 TIFF/PNG，
    // 位图优先会把图标误判为图片。图片扩展名读文件转 image，其余归类 file（忽略图标位图）。
    // 无 file URL 才看 PNG/TIFF（截图、浏览器复制图片），最后 text。
    let file_urls = pasteboard::read_file_urls();
    if !file_urls.is_empty() {
        // 批量复制超 10 个直接丢弃整批：避免大量 base64 同驻内存 + 主线程逐文件解码阻塞
        if file_urls.len() > 10 {
            return Vec::new();
        }
        for s in file_urls {
            // Finder 写入 file reference URL（file:///.file/id=...），需经 NSURL 解析为实际路径
            let path = pasteboard::resolve_file_url_to_path(&s).unwrap_or_else(|| {
                let stripped = s.strip_prefix("file://").unwrap_or(&s);
                percent_decode(stripped)
            });
            if let Some(img) = read_image_file(&path) {
                snaps.push(ClipboardSnapshot {
                    content: img.data_url,
                    content_type: "image".to_string(),
                    file_size: img.size,
                    image_width: img.width,
                    image_height: img.height,
                    source_app: String::new(),
                });
            } else {
                let mut file_size = None;
                if let Ok(meta) = std::fs::metadata(&path) {
                    file_size = Some(meta.len().min(i32::MAX as u64) as i32);
                }
                snaps.push(ClipboardSnapshot {
                    content: format!("file://{}", path),
                    content_type: "file".to_string(),
                    file_size,
                    image_width: None,
                    image_height: None,
                    source_app: String::new(),
                });
            }
        }
    } else if let Some(slice) = pasteboard::read_png().or_else(pasteboard::read_tiff_as_png) {
        let len = slice.len();
        if len > 0 {
            let (mut image_width, mut image_height) = (None, None);
            if len >= 24 && slice[0..4] == [0x89, 0x50, 0x4E, 0x47] {
                image_width = Some(u32::from_be_bytes(slice[16..20].try_into().unwrap()) as i32);
                image_height = Some(u32::from_be_bytes(slice[20..24].try_into().unwrap()) as i32);
            }
            snaps.push(ClipboardSnapshot {
                content: format!("data:image/png;base64,{}", base64.encode(&slice)),
                content_type: "image".to_string(),
                file_size: Some((len as u64).min(i32::MAX as u64) as i32),
                image_width,
                image_height,
                source_app: String::new(),
            });
        }
    } else if let Some(text) = pasteboard::read_text() {
        let text = text.trim().to_string();
        if !text.is_empty() && !is_all_emoji(&text) {
            snaps.push(ClipboardSnapshot {
                content: text,
                content_type: "text".to_string(),
                file_size: None,
                image_width: None,
                image_height: None,
                source_app: String::new(),
            });
        }
    }

    if snaps.is_empty() {
        return Vec::new();
    }

    // source_app（同一次复制共享同一来源）
    // source marker 标识 Voidnix 内部写入（pasteboard_write_text）；外部复制用 frontmostApplication
    // （Voidnix 是 accessory app，frontmostApplication 不返回自身，故外部复制能取真实来源）
    let source_app = if pasteboard::has_type("com.litiantao.voidnix.source") {
        "Voidnix".to_string()
    } else {
        let mut app = String::from("Unknown");
        let ws = NSWorkspace::sharedWorkspace();
        if let Some(front) = ws.frontmostApplication() {
            if let Some(name) = front.localizedName() {
                app = name.to_string();
            }
        }
        app
    };

    // secret 检查（text 类型，兜底 ConcealedType 未设的密码管理器）+ 填充 source_app
    snaps
        .into_iter()
        .filter(|s| {
            !(s.content_type == "text"
                && (is_password_manager(&source_app) || looks_like_secret(&s.content)))
        })
        .map(|mut s| {
            s.source_app = source_app.clone();
            s
        })
        .collect()
}

/// 纯 emoji 判定（跳过入库，避免噪声）。
fn is_all_emoji(text: &str) -> bool {
    text.chars().all(|c| {
        let cp = c as u32;
        (0x1F300..=0x1FAFF).contains(&cp)
            || (0x2600..=0x27BF).contains(&cp)
            || (0x2300..=0x23FF).contains(&cp)
            || (0x25A0..=0x25FF).contains(&cp)
            || (0x2190..=0x21FF).contains(&cp)
            || (0x2B00..=0x2BFF).contains(&cp)
            || (0xFE00..=0xFE0F).contains(&cp)
    })
}

/// 单条 snapshot 入库：已存在则 UPDATE 刷新（保留 favorite），否则 INSERT。
fn process_snapshot(conn: &Connection, snap: &ClipboardSnapshot, id: &str) {
    // 一次查询拿存在性 + favorite（ORDER BY is_favorite DESC LIMIT 1：有 favorite 优先返回 1）
    let existing: Option<bool> = conn
        .prepare("SELECT is_favorite FROM clipboard_history WHERE content = ?1 ORDER BY is_favorite DESC LIMIT 1")
        .ok()
        .and_then(|mut stmt| {
            stmt.query_row(rusqlite::params![snap.content], |row| row.get(0)).ok()
        });

    if let Some(is_favorite) = existing {
        let _ = conn.execute(
            "UPDATE clipboard_history SET created_at = datetime('now', 'localtime'), source_app = ?2, file_size = ?3, image_width = ?4, image_height = ?5 WHERE content = ?1",
            rusqlite::params![snap.content, snap.source_app, snap.file_size, snap.image_width, snap.image_height],
        );
        if is_favorite {
            let _ = conn.execute(
                "UPDATE clipboard_history SET is_favorite = 1 WHERE content = ?1",
                rusqlite::params![snap.content],
            );
        }
    } else {
        let _ = conn.execute(
            "INSERT INTO clipboard_history (id, content, content_type, source_app, is_favorite, file_size, image_width, image_height, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, datetime('now', 'localtime'))",
            rusqlite::params![id, snap.content, snap.content_type, snap.source_app, false, snap.file_size, snap.image_width, snap.image_height],
        );
    }
}

fn percent_decode(s: &str) -> String {
    let mut result = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) =
                u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16)
            {
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

/// file URL 指向图片文件时读为 base64 data URL（PNG/JPEG/GIF/WebP/BMP/HEIC），
/// 用于 Finder 复制图片文件等只写 file URL、不写 PNG data 的场景。
struct ImageFileData {
    data_url: String,
    width: Option<i32>,
    height: Option<i32>,
    size: Option<i32>,
}

/// 图片文件转 image 入库的大小上限：超限不转 image，归类 file 保留路径（避免内存/DB 膨胀）。
const MAX_IMAGE_FILE_SIZE: u64 = 10 * 1024 * 1024;

fn read_image_file(path: &str) -> Option<ImageFileData> {
    let ext = path.rsplit('.').next()?.to_ascii_lowercase();
    let mime = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "heic" | "heif" => "image/heic",
        _ => return None,
    };
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() > MAX_IMAGE_FILE_SIZE {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    if bytes.is_empty() {
        return None;
    }
    let len = bytes.len() as u64;
    let size = Some(len.min(i32::MAX as u64) as i32);
    let (width, height) = image_dims_from_bytes(&bytes);
    let data_url = format!("data:{};base64,{}", mime, base64.encode(&bytes));
    Some(ImageFileData {
        data_url,
        width,
        height,
        size,
    })
}

/// 从图片字节解析宽高（NSImage 统一解码，支持 PNG/JPEG/GIF/WebP/BMP/HEIC 等所有系统格式）。
/// 须在主线程调用（AppKit）。
fn image_dims_from_bytes(bytes: &[u8]) -> (Option<i32>, Option<i32>) {
    use objc2::rc::autoreleasepool;
    use objc2::AnyThread;
    use objc2_app_kit::NSImage;
    use objc2_foundation::NSData;
    autoreleasepool(|_| {
        let data = NSData::with_bytes(bytes);
        let image = NSImage::initWithData(NSImage::alloc(), &data)?;
        let size = image.size();
        let w = size.width as i32;
        let h = size.height as i32;
        if w > 0 && h > 0 {
            Some((Some(w), Some(h)))
        } else {
            None
        }
    })
    .unwrap_or((None, None))
}
