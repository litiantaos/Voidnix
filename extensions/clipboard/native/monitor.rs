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

/// 主线程轻采样结果：只触 AppKit pasteboard，不做 fs::read / base64 / 大图解码。
enum PasteboardSample {
    /// 首次轮询：只建立 changeCount 基准
    Baseline,
    /// changeCount 未变
    Unchanged,
    /// 自身 marker / 密码管理器 transient 等 → 跳过
    Skip,
    /// 有变化但无可入库内容
    Empty,
    Files {
        paths: Vec<String>,
        source_app: String,
    },
    ImagePng {
        bytes: Vec<u8>,
        source_app: String,
    },
    Text {
        text: String,
        source_app: String,
    },
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

/// 启发式判定：内容本身像 secret（赋值前缀 / PEM / 未知来源高熵 token）。
/// 保守策略——避免误伤正常代码/长串；长 base64 仅在 source 未知时收紧拦截。
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

/// 未知来源下的高熵独立 token（无空白、40–200 长、base64/hex 字符集 + 大小写数字混合）。
/// 故意窄：有已知 source_app 时不启用，降低代码/长 ID 误杀。
fn looks_like_high_entropy_token(text: &str) -> bool {
    let t = text.trim();
    if t.len() < 40 || t.len() > 200 || t.chars().any(|c| c.is_whitespace()) {
        return false;
    }
    if !t
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=' | '-' | '_'))
    {
        return false;
    }
    let has_digit = t.chars().any(|c| c.is_ascii_digit());
    let has_lower = t.chars().any(|c| c.is_ascii_lowercase());
    let has_upper = t.chars().any(|c| c.is_ascii_uppercase());
    has_digit && has_lower && has_upper
}

fn is_password_manager(app_name: &str) -> bool {
    let lower = app_name.to_ascii_lowercase();
    PASSWORD_MANAGER_APPS
        .iter()
        .any(|&name| lower == name || lower.contains(name))
}

fn should_drop_text(text: &str, source_app: &str) -> bool {
    if is_password_manager(source_app) || looks_like_secret(text) {
        return true;
    }
    // 仅 Unknown 来源启用高熵 token 拦截（有明确 app 名时信任 ConcealedType + app 列表）
    source_app == "Unknown" && looks_like_high_entropy_token(text)
}

pub fn start_monitor(app_handle: AppHandle) {
    std::thread::spawn(move || {
        // None = 首次轮询基准未建立（只取当前 changeCount，不读取内容），
        // 避免启动时把当前剪贴板已有内容入库一条。
        let mut last_change_count: Option<isize> = None;
        // 过期清理限频：每 N 次实际写入才跑一次 DELETE 扫描（非每次写入）
        const CLEANUP_INTERVAL: u32 = 50;
        let mut since_cleanup: u32 = 0;
        // channel 循环外创建一次复用，避免每轮分配（Sender 是 Clone，每轮 clone 进闭包）
        let (tx, rx) = std::sync::mpsc::channel::<(isize, PasteboardSample)>();

        loop {
            std::thread::sleep(Duration::from_millis(800));

            let tx = tx.clone();

            let last_for_closure = last_change_count;
            let _ = app_handle.run_on_main_thread(move || {
                use crate::platform::pasteboard;

                let change_count = pasteboard::change_count();

                let sample = match last_for_closure {
                    // 首次轮询：只建立基准，不读取内容
                    None => PasteboardSample::Baseline,
                    Some(last) if change_count == last => PasteboardSample::Unchanged,
                    Some(_) => sample_pasteboard_light(),
                };

                let _ = tx.send((change_count, sample));
            });

            // recv_timeout 而非 recv：channel 复用后外层 tx 始终保活，
            // 主线程闭包因故未执行时 recv 会永久阻塞；超时让监控线程能跳过本轮自愈。
            let Ok((new_change_count, sample)) = rx.recv_timeout(Duration::from_secs(5)) else {
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

            // 重活（fs::read / base64 / 可选 HEIC→PNG）在监控线程，不阻塞主线程
            let snaps = match sample {
                PasteboardSample::Baseline
                | PasteboardSample::Unchanged
                | PasteboardSample::Skip
                | PasteboardSample::Empty => Vec::new(),
                other => expand_sample(other, &app_handle),
            };

            if snaps.is_empty() {
                continue;
            }

            // setup 降级时未 manage Database：跳过本轮写入
            let Some(db) = app_handle.try_state::<Database>() else {
                continue;
            };
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
                .unwrap_or_default()
                .as_millis();
            for (i, snap) in snaps.iter().enumerate() {
                let id = format!("{}", base + i as u128);
                process_snapshot(&conn, snap, &id);
            }

            // 过期清理限频：每 CLEANUP_INTERVAL 次写入才执行一次 DELETE 扫描，
            // 避免频繁复制时每次写入都跑全表过期查询
            since_cleanup += 1;
            if since_cleanup >= CLEANUP_INTERVAL {
                since_cleanup = 0;
                let max_days: i32 = crate::extensions::clipboard::load_max_days();
                run_expiry_cleanup(&conn, max_days);
            }

            let _ = app_handle.emit("clipboard-updated", ());

            // 写入计数 + WAL checkpoint（防止 clipboard.db-wal 无限增长）
            db.maybe_checkpoint(&conn);
        }
    });
}

/// 过期清理：按 max_days 删除过期非收藏行，或超 MAX_ROWS 时裁剪最旧行。
/// 统一 UTC（与表默认 CURRENT_TIMESTAMP 一致，避免 localtime 混比 skew）。
fn run_expiry_cleanup(conn: &Connection, max_days: i32) {
    const MAX_ROWS: i64 = 5000;
    if max_days > 0 {
        let _ = conn.execute(
            "DELETE FROM clipboard_history WHERE is_favorite = 0 AND created_at < datetime('now', ?1)",
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
}

/// 主线程轻采样：marker / file URL 解析 / pasteboard 位图或文本。不做磁盘读与 base64。
fn sample_pasteboard_light() -> PasteboardSample {
    use crate::platform::pasteboard;

    // 跳过自身防回环 marker + transient/concealed/auto_gen（密码管理器）
    if pasteboard::has_type("com.litiantao.voidnix.clipboard")
        || pasteboard::has_type("org.nspasteboard.TransientType")
        || pasteboard::has_type("org.nspasteboard.ConcealedType")
        || pasteboard::has_type("org.nspasteboard.AutoGeneratedType")
    {
        return PasteboardSample::Skip;
    }

    let source_app = resolve_source_app();

    // file URL 优先（多文件）：访达复制文件常带图标 TIFF/PNG，位图优先会误判。
    // 此处只解析路径，读盘/解码放到 expand_sample。
    let file_urls = pasteboard::read_file_urls();
    if !file_urls.is_empty() {
        // 批量复制超 10 个直接丢弃整批
        if file_urls.len() > 10 {
            return PasteboardSample::Empty;
        }
        let mut paths = Vec::with_capacity(file_urls.len());
        for s in file_urls {
            let path = pasteboard::resolve_file_url_to_path(&s).unwrap_or_else(|| {
                let stripped = s.strip_prefix("file://").unwrap_or(&s);
                percent_decode(stripped)
            });
            paths.push(path);
        }
        return PasteboardSample::Files { paths, source_app };
    }

    if let Some(slice) = pasteboard::read_png(MAX_IMAGE_FILE_SIZE)
        .or_else(|| pasteboard::read_tiff_as_png(MAX_IMAGE_FILE_SIZE))
    {
        return PasteboardSample::ImagePng {
            bytes: slice,
            source_app,
        };
    }

    if let Some(text) = pasteboard::read_text() {
        let text = text.trim().to_string();
        if !text.is_empty() && !is_all_emoji(&text) {
            return PasteboardSample::Text { text, source_app };
        }
    }

    PasteboardSample::Empty
}

fn resolve_source_app() -> String {
    use crate::platform::pasteboard;
    use objc2_app_kit::NSWorkspace;

    // source marker 标识 Voidnix 内部写入；外部复制用 frontmostApplication
    if pasteboard::has_type("com.litiantao.voidnix.source") {
        return "Voidnix".to_string();
    }
    let mut app = String::from("Unknown");
    let ws = NSWorkspace::sharedWorkspace();
    if let Some(front) = ws.frontmostApplication() {
        if let Some(name) = front.localizedName() {
            app = name.to_string();
        }
    }
    app
}

/// 监控线程展开：磁盘读 / base64 / HEIC→PNG（HEIC 编码经主线程 AppKit）。
fn expand_sample(sample: PasteboardSample, app: &AppHandle) -> Vec<ClipboardSnapshot> {
    match sample {
        PasteboardSample::Files { paths, source_app } => {
            let mut snaps = Vec::with_capacity(paths.len());
            for path in paths {
                if let Some(img) = read_image_file(&path, app) {
                    snaps.push(ClipboardSnapshot {
                        content: img.data_url,
                        content_type: "image".to_string(),
                        file_size: img.size,
                        image_width: img.width,
                        image_height: img.height,
                        source_app: source_app.clone(),
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
                        source_app: source_app.clone(),
                    });
                }
            }
            snaps
        }
        PasteboardSample::ImagePng { bytes, source_app } => {
            let len = bytes.len();
            let (image_width, image_height) = png_dims(&bytes);
            vec![ClipboardSnapshot {
                content: format!("data:image/png;base64,{}", base64.encode(&bytes)),
                content_type: "image".to_string(),
                file_size: Some((len as u64).min(i32::MAX as u64) as i32),
                image_width,
                image_height,
                source_app,
            }]
        }
        PasteboardSample::Text { text, source_app } => {
            if should_drop_text(&text, &source_app) {
                return Vec::new();
            }
            vec![ClipboardSnapshot {
                content: text,
                content_type: "text".to_string(),
                file_size: None,
                image_width: None,
                image_height: None,
                source_app,
            }]
        }
        PasteboardSample::Baseline
        | PasteboardSample::Unchanged
        | PasteboardSample::Skip
        | PasteboardSample::Empty => Vec::new(),
    }
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
/// 时间一律 UTC（`datetime('now')`），与 DEFAULT CURRENT_TIMESTAMP 一致。
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
        let _ = conn.execute(
            "INSERT INTO clipboard_history (id, content, content_type, source_app, is_favorite, file_size, image_width, image_height, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, datetime('now'))",
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

/// file URL 指向图片文件时读为 base64 data URL。
/// HEIC/HEIF 归一化为 PNG（WKWebView 可预览）；其余格式保留原 codec（均可预览）。
struct ImageFileData {
    data_url: String,
    width: Option<i32>,
    height: Option<i32>,
    size: Option<i32>,
}

/// 图片文件转 image 入库的大小上限：超限不转 image，归类 file 保留路径（避免内存/DB 膨胀）。
const MAX_IMAGE_FILE_SIZE: u64 = 10 * 1024 * 1024;

fn read_image_file(path: &str, app: &AppHandle) -> Option<ImageFileData> {
    let ext = path.rsplit('.').next()?.to_ascii_lowercase();
    let (mime, needs_png) = match ext.as_str() {
        "png" => ("image/png", false),
        "jpg" | "jpeg" => ("image/jpeg", false),
        "gif" => ("image/gif", false),
        "webp" => ("image/webp", false),
        "bmp" => ("image/bmp", false),
        // HEIC 在 WKWebView 无法作 data URL 预览 → 主线程 encode 为 PNG
        "heic" | "heif" => ("image/png", true),
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

    if needs_png {
        let png = encode_png_on_main(app, bytes)?;
        let len = png.len() as u64;
        let (width, height) = png_dims(&png);
        return Some(ImageFileData {
            data_url: format!("data:image/png;base64,{}", base64.encode(&png)),
            width,
            height,
            size: Some(len.min(i32::MAX as u64) as i32),
        });
    }

    let len = bytes.len() as u64;
    let size = Some(len.min(i32::MAX as u64) as i32);
    let (width, height) = image_dims_from_headers(&bytes, mime);
    let data_url = format!("data:{};base64,{}", mime, base64.encode(&bytes));
    Some(ImageFileData {
        data_url,
        width,
        height,
        size,
    })
}

/// HEIC→PNG 必须走 AppKit NSImage，回到主线程编码。
fn encode_png_on_main(app: &AppHandle, bytes: Vec<u8>) -> Option<Vec<u8>> {
    let (tx, rx) = std::sync::mpsc::channel();
    let _ = app.run_on_main_thread(move || {
        let _ = tx.send(crate::platform::pasteboard::encode_image_to_png(&bytes));
    });
    rx.recv().ok().flatten()
}

fn png_dims(bytes: &[u8]) -> (Option<i32>, Option<i32>) {
    if bytes.len() >= 24 && bytes[0..4] == [0x89, 0x50, 0x4E, 0x47] {
        let w = u32::from_be_bytes(bytes[16..20].try_into().unwrap()) as i32;
        let h = u32::from_be_bytes(bytes[20..24].try_into().unwrap()) as i32;
        if w > 0 && h > 0 {
            return (Some(w), Some(h));
        }
    }
    (None, None)
}

/// 无 AppKit 的轻量宽高解析（监控线程安全）。
fn image_dims_from_headers(bytes: &[u8], mime: &str) -> (Option<i32>, Option<i32>) {
    match mime {
        "image/png" => png_dims(bytes),
        "image/gif" if bytes.len() >= 10 && bytes.starts_with(b"GIF") => {
            let w = u16::from_le_bytes([bytes[6], bytes[7]]) as i32;
            let h = u16::from_le_bytes([bytes[8], bytes[9]]) as i32;
            if w > 0 && h > 0 {
                (Some(w), Some(h))
            } else {
                (None, None)
            }
        }
        "image/jpeg" => jpeg_dims(bytes),
        "image/bmp" if bytes.len() >= 26 && bytes[0] == b'B' && bytes[1] == b'M' => {
            let w = i32::from_le_bytes(bytes[18..22].try_into().unwrap());
            let h = i32::from_le_bytes(bytes[22..26].try_into().unwrap()).abs();
            if w > 0 && h > 0 {
                (Some(w), Some(h))
            } else {
                (None, None)
            }
        }
        _ => (None, None),
    }
}

fn jpeg_dims(bytes: &[u8]) -> (Option<i32>, Option<i32>) {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return (None, None);
    }
    let mut i = 2usize;
    while i + 9 < bytes.len() {
        if bytes[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = bytes[i + 1];
        // SOF0–SOF3 / SOF5–SOF7 / SOF9–SOF11 / SOF13–SOF15
        if matches!(
            marker,
            0xC0 | 0xC1
                | 0xC2
                | 0xC3
                | 0xC5
                | 0xC6
                | 0xC7
                | 0xC9
                | 0xCA
                | 0xCB
                | 0xCD
                | 0xCE
                | 0xCF
        ) {
            let h = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as i32;
            let w = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]) as i32;
            if w > 0 && h > 0 {
                return (Some(w), Some(h));
            }
            return (None, None);
        }
        if marker == 0xD9 || marker == 0xDA {
            break;
        }
        if i + 3 >= bytes.len() {
            break;
        }
        let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
        if len < 2 {
            break;
        }
        i += 2 + len;
    }
    (None, None)
}
