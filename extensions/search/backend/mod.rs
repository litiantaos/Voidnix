use nucleo_matcher::{
    pattern::{CaseMatching, Normalization, Pattern},
    Matcher,
};
use pinyin::ToPinyin;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::sync::RwLock;
use once_cell::sync::Lazy;
use tauri::Emitter;

thread_local! {
    static MATCHER: RefCell<Matcher> = RefCell::new(Matcher::default());
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub path: String,
    pub kind: String,
    pub icon: Option<String>,
    pub last_used: Option<String>,
    pub score: Option<i32>,
}

use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Debug)]
struct CachedApp {
    name: String,
    bundle_name: String,
    pinyin_full: String,
    pinyin_initials: String,
    pinyin_compact: String,
    path: String,
    icon_cache: Option<String>,
    last_used: Option<String>,
    use_count: AtomicU32,
}

static APP_CACHE: Lazy<RwLock<Option<Arc<Vec<CachedApp>>>>> = Lazy::new(|| RwLock::new(None));
static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

use std::collections::HashMap;
use std::sync::Mutex;

struct SearchSession {
    search_id: AtomicU32,
    session_use_deltas: Mutex<HashMap<String, u32>>,
}

impl SearchSession {
    fn new() -> Self {
        Self {
            search_id: AtomicU32::new(0),
            session_use_deltas: Mutex::new(HashMap::new()),
        }
    }

    fn next_search_id(&self) -> u32 {
        self.search_id.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn get_current_id(&self) -> u32 {
        self.search_id.load(Ordering::SeqCst)
    }

    fn increment_use_count(&self, path: &str) {
        if let Ok(mut deltas) = self.session_use_deltas.lock() {
            *deltas.entry(path.to_string()).or_insert(0) += 1;
        }
    }

    fn take_deltas(&self) -> HashMap<String, u32> {
        self.session_use_deltas.lock()
            .map(|mut d| std::mem::take(&mut *d))
            .unwrap_or_default()
    }
}

static SEARCH_SESSION: Lazy<SearchSession> = Lazy::new(SearchSession::new);

use objc2::{runtime::AnyObject, AnyThread};
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSCompositingOperation, NSGraphicsContext, NSImage,
    NSImageInterpolation, NSWorkspace,
};
use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize, NSString};

/// 获取 app bundle 的修改时间（秒级 unix timestamp），用于缓存失效。
/// 使用 bundle 目录本身的 mtime：macOS 更新应用时会更新目录 mtime，
/// 但不一定更新 Info.plist 的 mtime，所以不能依赖 Info.plist。
fn get_bundle_mtime(app_path: &str) -> u64 {
    fs::metadata(app_path)
        .and_then(|m| m.modified())
        .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs())
        .unwrap_or(0)
}

/// 计算图标缓存文件路径，key = hash(app_path + mtime)，mtime 变化时自动失效
fn icon_cache_path(cache_dir: &Path, app_path: &str) -> std::path::PathBuf {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mtime = get_bundle_mtime(app_path);
    let mut hasher = DefaultHasher::new();
    app_path.hash(&mut hasher);
    mtime.hash(&mut hasher);
    let hash = hasher.finish();
    cache_dir.join(format!("{}.png", hash))
}

fn get_app_icon(app_path: &str) -> Option<String> {
    let cache_dir = dirs::cache_dir()
        .unwrap_or_else(|| Path::new("/tmp").to_path_buf())
        .join("Launcher")
        .join("icons");

    let _ = fs::create_dir_all(&cache_dir);

    let cached_path = icon_cache_path(&cache_dir, app_path);

    if cached_path.exists() {
        if let Ok(bytes) = fs::read(&cached_path) {
            if !bytes.is_empty() {
                log::debug!("[icon] Cache hit for: {}", app_path);
                return Some(base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    &bytes,
                ));
            }
        }
        // 缓存文件存在但为空或读取失败，删除后重新提取
        let _ = fs::remove_file(&cached_path);
    }

    match extract_app_icon(app_path) {
        Some(base64_str) => {
            log::debug!("Icon extracted for: {}", app_path);
            Some(base64_str)
        }
        None => {
            log::warn!("Failed to extract icon for: {}", app_path);
            None
        }
    }
}

/// 从 macOS 提取应用图标：先用 NSWorkspace 获取系统样式图标，失败后才注册到 Launch Services 重试
fn extract_app_icon(app_path: &str) -> Option<String> {
    let cache_dir = dirs::cache_dir()
        .unwrap_or_else(|| Path::new("/tmp").to_path_buf())
        .join("Launcher")
        .join("icons");

    let cached_path = icon_cache_path(&cache_dir, app_path);

    // 先直接尝试 NSWorkspace，大多数已安装应用无需预注册
    if let Some(base64_str) = extract_icon_via_workspace(app_path, &cached_path, false) {
        return Some(base64_str);
    }

    // NSWorkspace 首次失败：注册到 Launch Services 后重试一次（仅针对新安装应用）
    log::info!("[icon] NSWorkspace failed for {}, registering with LS and retrying", app_path);
    register_app_with_ls(app_path);
    if let Some(base64_str) = extract_icon_via_workspace(app_path, &cached_path, true) {
        return Some(base64_str);
    }

    // 最终降级：从 bundle 的 Info.plist 直接读取 icns，手动添加内边距
    log::info!("[icon] NSWorkspace failed after LS registration for {}, trying bundle icns", app_path);
    if let Some(base64_str) = extract_icon_from_bundle(app_path, &cached_path) {
        return Some(base64_str);
    }

    log::error!("[icon] All extraction methods failed for: {}", app_path);
    None
}

/// 使用 lsregister 强制注册应用到 Launch Services（仅在 NSWorkspace 失败时按需调用）
fn register_app_with_ls(app_path: &str) {
    let lsregister = "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister";
    if Path::new(lsregister).exists() {
        let child = std::process::Command::new(lsregister)
            .arg("-f")
            .arg(app_path)
            .spawn();
        match child {
            Ok(mut c) => {
                match c.wait() {
                    Ok(s) if s.success() => {
                        log::debug!("lsregister succeeded for: {}", app_path);
                    }
                    Ok(s) => {
                        log::warn!("lsregister exited with {} for: {}", s, app_path);
                    }
                    Err(e) => {
                        log::warn!("lsregister wait failed for {}: {}", app_path, e);
                    }
                }
            }
            Err(e) => {
                log::warn!("lsregister spawn failed: {}", e);
            }
        }
    }
}

/// 通过 NSWorkspace 提取图标（自动带系统样式）
/// after_ls_register=true 时等待 300ms 让 LS 数据库刷新
fn extract_icon_via_workspace(app_path: &str, cached_path: &Path, after_ls_register: bool) -> Option<String> {
    if after_ls_register {
        std::thread::sleep(std::time::Duration::from_millis(300));
    }

    let result = unsafe {
        let path_str = NSString::from_str(app_path);
        let workspace = NSWorkspace::sharedWorkspace();

        #[allow(deprecated)]
        let icon = workspace.iconForFile(&path_str);
        nsimage_to_png_base64(&icon, cached_path)
    };

    if result.is_some() {
        log::debug!("[icon] NSWorkspace extracted for: {}", app_path);
    }
    result
}

/// 直接从 app bundle 的 Info.plist -> CFBundleIconFile 读取 icns 并转为 PNG
/// 使用 plist crate 解析，避免子进程开销
fn extract_icon_from_bundle(app_path: &str, cached_path: &Path) -> Option<String> {
    let plist_path = Path::new(app_path).join("Contents").join("Info.plist");
    if !plist_path.exists() {
        log::warn!("[icon] Info.plist not found: {:?}", plist_path);
        return None;
    }

    let dict = plist::Value::from_file(&plist_path)
        .ok()
        .and_then(|v| v.into_dictionary())?;

    // 优先 CFBundleIconFile（直接 icns 文件），其次检查 CFBundleIconName（Asset Catalog，无法直接读取）
    let icon_name = match dict.get("CFBundleIconFile").and_then(|v| v.as_string()) {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => {
            if let Some(asset_name) = dict.get("CFBundleIconName").and_then(|v| v.as_string()) {
                log::info!("[icon] {} uses Asset Catalog icon: {}", app_path, asset_name);
            } else {
                log::warn!("[icon] No CFBundleIconFile/CFBundleIconName in {}", app_path);
            }
            return None;
        }
    };

    // CFBundleIconFile 可能带也可能不带 .icns 后缀
    let icon_name = if icon_name.ends_with(".icns") {
        icon_name
    } else {
        format!("{}.icns", icon_name)
    };

    let resources_dir = Path::new(app_path).join("Contents").join("Resources");
    let icon_path = resources_dir.join(&icon_name);

    if !icon_path.exists() {
        // 尝试在 Resources 子目录中搜索
        let found = find_icns_in_resources(&resources_dir, &icon_name);
        match found {
            Some(path) => return convert_icns_to_png(&path, cached_path, app_path),
            None => {
                let icns_files: Vec<String> = fs::read_dir(&resources_dir)
                    .ok()
                    .into_iter()
                    .flatten()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().map_or(false, |ext| ext == "icns"))
                    .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
                    .collect();
                log::warn!("[icon] Icon not found: {:?}, available: {:?}", icon_path, icns_files);
                return None;
            }
        }
    }

    convert_icns_to_png(&icon_path, cached_path, app_path)
}

/// 在 Resources 目录中搜索 icns 文件
fn find_icns_in_resources(resources_dir: &Path, icon_name: &str) -> Option<std::path::PathBuf> {
    for entry in fs::read_dir(resources_dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_icns_in_resources(&path, icon_name) {
                return Some(found);
            }
        } else if path.file_name().and_then(|n| n.to_str()) == Some(icon_name) {
            return Some(path);
        }
    }
    None
}

/// 将 icns 文件转换为 PNG base64（添加系统风格内边距）
fn convert_icns_to_png(icon_path: &Path, cached_path: &Path, app_path: &str) -> Option<String> {
    let icon_path_str = icon_path.to_str()?;

    // 方法 1: NSImage 直接加载，添加内边距模拟系统图标样式
    unsafe {
        let path_ns = NSString::from_str(icon_path_str);
        if let Some(image) = NSImage::initWithContentsOfFile(NSImage::alloc(), &path_ns) {
            if let Some(result) = nsimage_to_png_base64_with_padding(&image, cached_path, true) {
                log::info!("[icon] Extracted via NSImage with padding: {}", app_path);
                return Some(result);
            }
        }
    }

    // 方法 2: sips 命令行转换
    log::info!("[icon] NSImage failed, trying sips for: {}", app_path);

    // 先用 sips 转到临时文件，再加载并添加内边距
    let tmp_path = cached_path.with_extension("tmp.png");
    let output = std::process::Command::new("sips")
        .arg("-s")
        .arg("format")
        .arg("png")
        .arg(icon_path_str)
        .arg("--out")
        .arg(&tmp_path)
        .output()
        .ok()?;

    if output.status.success() {
        unsafe {
            let tmp_ns = NSString::from_str(tmp_path.to_str()?);
            if let Some(image) = NSImage::initWithContentsOfFile(NSImage::alloc(), &tmp_ns) {
                let result = nsimage_to_png_base64_with_padding(&image, cached_path, true);
                let _ = fs::remove_file(&tmp_path);
                if result.is_some() {
                    log::info!("[icon] Extracted via sips with padding: {}", app_path);
                }
                return result;
            }
        }
        let _ = fs::remove_file(&tmp_path);
    } else {
        log::warn!("[icon] sips failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    None
}


/// 将 NSImage 转换为 64x64 PNG base64 字符串
unsafe fn nsimage_to_png_base64(image: &NSImage, cached_path: &Path) -> Option<String> {
    nsimage_to_png_base64_with_padding(image, cached_path, false)
}

/// 将 NSImage 转换为 64x64 PNG base64 字符串，可选择添加系统风格内边距
unsafe fn nsimage_to_png_base64_with_padding(
    image: &NSImage,
    cached_path: &Path,
    add_padding: bool,
) -> Option<String> {
    let canvas_size = NSSize::new(64.0, 64.0);
    let new_image = NSImage::initWithSize(NSImage::alloc(), canvas_size);

    #[allow(deprecated)]
    new_image.lockFocus();

    if let Some(ctx) = NSGraphicsContext::currentContext() {
        ctx.setImageInterpolation(NSImageInterpolation(3));
    }

    // macOS 系统图标约有 15% 的内边距
    let (dest_rect, src_rect) = if add_padding {
        let padding = 64.0 * 0.15; // ~10px padding on each side
        let icon_size = 64.0 - padding * 2.0;
        (
            NSRect::new(NSPoint::new(padding, padding), NSSize::new(icon_size, icon_size)),
            NSRect::new(NSPoint::new(0.0, 0.0), image.size()),
        )
    } else {
        (
            NSRect::new(NSPoint::new(0.0, 0.0), canvas_size),
            NSRect::new(NSPoint::new(0.0, 0.0), image.size()),
        )
    };

    image.drawInRect_fromRect_operation_fraction(
        dest_rect,
        src_rect,
        NSCompositingOperation::Copy,
        1.0,
    );

    #[allow(deprecated)]
    new_image.unlockFocus();

    let tiff_data = new_image.TIFFRepresentation()?;
    let bitmap_rep = NSBitmapImageRep::initWithData(NSBitmapImageRep::alloc(), &tiff_data)?;

    let empty_dict = NSDictionary::<NSString, AnyObject>::new();
    let png_data = bitmap_rep
        .representationUsingType_properties(NSBitmapImageFileType::PNG, &empty_dict)?;

    let bytes = png_data.to_vec();
    let _ = fs::write(cached_path, &bytes);

    Some(base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        &bytes,
    ))
}

fn get_app_metadata(app_path: &str) -> (String, Option<String>, u32) {
    let output = std::process::Command::new("mdls")
        .arg("-name")
        .arg("kMDItemDisplayName")
        .arg("-name")
        .arg("kMDItemLastUsedDate")
        .arg("-name")
        .arg("kMDItemUseCount")
        .arg(app_path)
        .output();

    let mut name = String::new();
    let mut last_used = None;
    let mut use_count = 0;

    if let Ok(out) = output {
        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines() {
            if line.starts_with("kMDItemDisplayName") {
                if let Some(val) = line.split('=').nth(1) {
                    let cleaned = val.trim().trim_matches('"').to_string();
                    name = if cleaned.ends_with(".app") {
                        cleaned.strip_suffix(".app").unwrap_or(&cleaned).to_string()
                    } else {
                        cleaned
                    };
                }
            } else if line.starts_with("kMDItemLastUsedDate") {
                if let Some(val) = line.split('=').nth(1) {
                    let cleaned = val.trim().to_string();
                    if cleaned != "(null)" {
                        last_used = Some(cleaned);
                    }
                }
            } else if line.starts_with("kMDItemUseCount") {
                if let Some(val) = line.split('=').nth(1) {
                    let cleaned = val.trim();
                    if let Ok(count) = cleaned.parse::<u32>() {
                        use_count = count;
                    }
                }
            }
        }
    }

    if name.is_empty() || name == "(null)" {
        name = get_app_name_from_plist(app_path)
            .unwrap_or_else(|| {
                std::path::Path::new(app_path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Unknown")
                    .to_string()
            });
    }

    (name, last_used, use_count)
}

/// 直接从 Info.plist 读取应用显示名称，作为 Spotlight 未索引时的回退
fn get_app_name_from_plist(app_path: &str) -> Option<String> {
    let plist_path = Path::new(app_path).join("Contents").join("Info.plist");
    let dict = plist::Value::from_file(&plist_path).ok()?.into_dictionary()?;
    // 优先 CFBundleDisplayName，其次 CFBundleName
    let name = dict
        .get("CFBundleDisplayName")
        .and_then(|v| v.as_string())
        .or_else(|| dict.get("CFBundleName").and_then(|v| v.as_string()))?;
    let name = name.to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// 递归扫描目录中的 .app 包，绕过 Spotlight 索引。max_depth 防止深层遍历。
fn scan_apps_from_dir(dir: &Path) -> Vec<String> {
    scan_apps_from_dir_depth(dir, 0)
}

fn scan_apps_from_dir_depth(dir: &Path, depth: u32) -> Vec<String> {
    if depth > 5 {
        return Vec::new();
    }
    let mut apps = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "app") {
                if let Some(path_str) = path.to_str() {
                    apps.push(path_str.to_string());
                }
            } else if path.is_dir() {
                apps.extend(scan_apps_from_dir_depth(&path, depth + 1));
            }
        }
    }
    apps
}

fn to_pinyin_full(name: &str) -> String {
    name.chars()
        .filter_map(|c| {
            if c.is_ascii() {
                Some(c.to_ascii_lowercase().to_string())
            } else {
                c.to_pinyin()
                    .map(|p| p.plain().to_string())
                    .or_else(|| Some(c.to_string()))
            }
        })
        .collect()
}

/// 多音字词库：字符级 pinyin crate 对多音字只能返回最常见读音（如 乐→lè），
/// 需要词级词典覆盖正确读音（如 音乐→yinyue）。仅对 macOS 常见应用名覆盖。
static PINYIN_WORDS: Lazy<Vec<(&str, &str)>> = Lazy::new(|| {
    vec![
        ("音乐", "yinyue"),
        ("相册", "xiangce"),
        ("长", "chang"),     // 长按→chang; default 长→zhang
        ("行", "hang"),      // 银行→hang; default 行→xing
        ("重命", "chongming"),
        ("地图", "ditu"),
    ]
});

fn word_pinyin_overrides(name: &str) -> String {
    PINYIN_WORDS
        .iter()
        .filter_map(|(word, pinyin)| {
            if name.contains(word) { Some(*pinyin) } else { None }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn to_pinyin_initials(name: &str) -> String {
    name.chars()
        .filter_map(|c| {
            if c.is_ascii() {
                Some(c.to_ascii_lowercase())
            } else {
                c.to_pinyin()
                    .map(|p| p.plain().chars().next().unwrap_or(c))
                    .or_else(|| Some(c)) // 即使不是拼音，也保留原字符（比如数字和符号）
            }
        })
        .collect()
}

fn parse_mdfind_attr_output(stdout: &str) -> Vec<(String, String, Option<String>, u32)> {
    let mut results: Vec<(String, String, Option<String>, u32)> = Vec::new();
    let mut current_path: Option<String> = None;
    let mut current_name = String::new();
    let mut current_last_used: Option<String> = None;
    let mut current_use_count: u32 = 0;

    let mut flush = |path: &mut Option<String>, name: &mut String, last_used: &mut Option<String>, use_count: &mut u32| {
        if let Some(p) = path.take() {
            if name.is_empty() || name == "(null)" {
                *name = std::path::Path::new(&p)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Unknown")
                    .to_string();
            }
            results.push((p, std::mem::take(name), last_used.take(), std::mem::take(use_count)));
        }
    };

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            flush(&mut current_path, &mut current_name, &mut current_last_used, &mut current_use_count);
            continue;
        }

        // mdfind -attr 输出格式：路径和属性在同一行，用空格分隔
        // 例：/path/to/app.app   kMDItemDisplayName = Name   kMDItemUseCount = 10
        if trimmed.starts_with('/') {
            flush(&mut current_path, &mut current_name, &mut current_last_used, &mut current_use_count);

            // 按属性分割：第一个是路径，后面是 kMDItemXxx = value
            let parts: Vec<&str> = trimmed.splitn(4, "   ").collect();
            current_path = Some(parts[0].trim().to_string());

            // 解析同一行上的属性
            for part in &parts[1..] {
                let part = part.trim();
                if part.starts_with("kMDItemDisplayName") {
                    if let Some(val) = part.split('=').nth(1) {
                        let cleaned = val.trim().trim_matches('"').to_string();
                        current_name = if cleaned.ends_with(".app") {
                            cleaned.strip_suffix(".app").unwrap_or(&cleaned).to_string()
                        } else {
                            cleaned
                        };
                    }
                } else if part.starts_with("kMDItemLastUsedDate") {
                    if let Some(val) = part.split('=').nth(1) {
                        let cleaned = val.trim().to_string();
                        if cleaned != "(null)" {
                            current_last_used = Some(cleaned);
                        }
                    }
                } else if part.starts_with("kMDItemUseCount") {
                    if let Some(val) = part.split('=').nth(1) {
                        if let Ok(count) = val.trim().parse::<u32>() {
                            current_use_count = count;
                        }
                    }
                }
            }
        } else if trimmed.starts_with("kMDItemDisplayName") {
            if let Some(val) = trimmed.split('=').nth(1) {
                let cleaned = val.trim().trim_matches('"').to_string();
                current_name = if cleaned.ends_with(".app") {
                    cleaned.strip_suffix(".app").unwrap_or(&cleaned).to_string()
                } else {
                    cleaned
                };
            }
        } else if trimmed.starts_with("kMDItemLastUsedDate") {
            if let Some(val) = trimmed.split('=').nth(1) {
                let cleaned = val.trim().to_string();
                if cleaned != "(null)" {
                    current_last_used = Some(cleaned);
                }
            }
        } else if trimmed.starts_with("kMDItemUseCount") {
            if let Some(val) = trimmed.split('=').nth(1) {
                if let Ok(count) = val.trim().parse::<u32>() {
                    current_use_count = count;
                }
            }
        }
    }
    flush(&mut current_path, &mut current_name, &mut current_last_used, &mut current_use_count);

    results
}

fn collect_apps_with_metadata() -> Vec<(String, String, Option<String>, u32)> {
    let search_dirs = [
        "/Applications",
        "/System/Applications",
        "/System/Library/CoreServices/Applications",
    ];

    let mut command = std::process::Command::new("mdfind");
    command.arg("kMDItemContentType == 'com.apple.application-bundle'");
    command.arg("-attr").arg("kMDItemDisplayName");
    command.arg("-attr").arg("kMDItemLastUsedDate");
    command.arg("-attr").arg("kMDItemUseCount");

    for dir in &search_dirs {
        command.arg("-onlyin").arg(dir);
    }

    if let Some(home_dir) = dirs::home_dir() {
        let home_apps = home_dir.join("Applications");
        if home_apps.exists() {
            command.arg("-onlyin").arg(&home_apps);
        }
    }

    let mut results = if let Ok(output) = command.output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_mdfind_attr_output(&stdout)
    } else {
        Vec::new()
    };

    let existing: std::collections::HashSet<String> = results.iter().map(|(p, _, _, _)| p.clone()).collect();

    let fs_scan_dirs: Vec<&str> = search_dirs.to_vec();
    let mut fs_apps: Vec<String> = Vec::new();
    for dir in &fs_scan_dirs {
        fs_apps.extend(scan_apps_from_dir(Path::new(dir)));
    }
    if let Some(home_dir) = dirs::home_dir() {
        let home_apps = home_dir.join("Applications");
        if home_apps.exists() {
            fs_apps.extend(scan_apps_from_dir(&home_apps));
        }
    }

    for path in fs_apps {
        if !existing.contains(&path) {
            log::info!("Found app via filesystem scan (not in mdfind): {}", path);
            let (name, last_used, use_count) = get_app_metadata(&path);
            results.push((path, name, last_used, use_count));
        }
    }

    let finder_path = "/System/Library/CoreServices/Finder.app".to_string();
    if Path::new(&finder_path).exists() && !existing.contains(&finder_path) {
        let (name, last_used, use_count) = get_app_metadata(&finder_path);
        results.push((finder_path, name, last_used, use_count));
    }

    results
}

fn get_bundle_name(app_path: &str) -> String {
    let plist_path = Path::new(app_path).join("Contents").join("Info.plist");
    plist::Value::from_file(&plist_path)
        .ok()
        .and_then(|v| v.into_dictionary())
        .and_then(|dict| {
            dict.get("CFBundleName")
                .and_then(|v| v.as_string())
                .map(|s| s.to_string())
        })
        .unwrap_or_default()
}

async fn init_app_cache() -> Arc<Vec<CachedApp>> {
    log::info!("Starting app cache initialization...");

    let app_metas = match tokio::task::spawn_blocking(collect_apps_with_metadata).await {
        Ok(metas) => metas,
        Err(e) => {
            log::error!("collect_apps_with_metadata panicked or was cancelled: {:?}", e);
            Vec::new()
        }
    };

    let session_deltas = SEARCH_SESSION.take_deltas();

    let mut apps = Vec::with_capacity(app_metas.len());

    for (path, name, last_used, system_use_count) in app_metas {
        let delta = session_deltas.get(&path).copied().unwrap_or(0);
        let bundle_name = get_bundle_name(&path);
        let pinyin_full = to_pinyin_full(&name);
        let pinyin_initials = to_pinyin_initials(&name);
        let pinyin_compact = pinyin_full.chars().filter(|c| !c.is_whitespace()).collect::<String>();

        apps.push(CachedApp {
            name,
            bundle_name,
            pinyin_full,
            pinyin_initials,
            pinyin_compact,
            path,
            icon_cache: None,
            last_used,
            use_count: AtomicU32::new(system_use_count + delta),
        });
    }

    let result = Arc::new(apps);
    log::info!("App cache initialized with {} apps (icons loading in background)", result.len());

    let result_clone = result.clone();
    let app_count = result.len();
    tokio::spawn(async move {
        let apps_with_icons = tokio::task::spawn_blocking(move || {
            // rayon 并发提取图标，自动按 CPU 核数分配线程
            // 始终通过 get_app_icon 走缓存路径：命中有效磁盘缓存则直接返回，
            // mtime 变化（应用更新）时旧 hash 文件不存在，会重新提取新图标
            result_clone
                .par_iter()
                .map(|app| {
                    let icon = get_app_icon(&app.path);
                    CachedApp {
                        name: app.name.clone(),
                        bundle_name: app.bundle_name.clone(),
                        pinyin_full: app.pinyin_full.clone(),
                        pinyin_initials: app.pinyin_initials.clone(),
                        pinyin_compact: app.pinyin_compact.clone(),
                        path: app.path.clone(),
                        icon_cache: icon,
                        last_used: app.last_used.clone(),
                        use_count: AtomicU32::new(app.use_count.load(Ordering::Relaxed)),
                    }
                })
                .collect()
        })
        .await
        .unwrap_or_default();

        let mut cache = APP_CACHE.write().await;
        *cache = Some(Arc::new(apps_with_icons));
        log::info!("Background icon loading complete for {} apps", app_count);

        if let Some(app) = APP_HANDLE.get() {
            let _ = app.emit("app-cache-updated", ());
        }
    });

    result
}

pub fn set_app_handle(handle: tauri::AppHandle) {
    let _ = APP_HANDLE.set(handle);
}

pub async fn prewarm_cache() {
    get_cached_apps().await;
}

async fn get_cached_apps() -> Arc<Vec<CachedApp>> {
    {
        let cache = APP_CACHE.read().await;
        if let Some(apps) = &*cache {
            return apps.clone();
        }
    }

    let apps = init_app_cache().await;

    {
        let mut cache = APP_CACHE.write().await;
        if let Some(existing) = &*cache {
            return existing.clone();
        }
        *cache = Some(apps.clone());
    }

    apps
}

pub fn init_app_watcher() {
    use notify::{Watcher, RecursiveMode, recommended_watcher};
    use std::time::Duration;
    use tokio::time::sleep;

    tauri::async_runtime::spawn(async {
        let (tx, mut rx) = tokio::sync::mpsc::channel(100);
        
        let watcher_res = recommended_watcher(move |res| {
            if let Ok(event) = res {
                let _ = tx.blocking_send(event);
            }
        });

        if let Ok(mut watcher) = watcher_res {
            let _ = watcher.watch(Path::new("/Applications"), RecursiveMode::NonRecursive);
            let _ = watcher.watch(Path::new("/System/Applications"), RecursiveMode::NonRecursive);
            if let Some(home) = dirs::home_dir() {
                let _ = watcher.watch(&home.join("Applications"), RecursiveMode::NonRecursive);
            }

            // 防抖逻辑：收集事件，如果一定时间内没有新事件，则触发重新构建
            loop {
                if rx.recv().await.is_some() {
                    // 等待 5 秒防抖，确保新安装应用的 bundle 完全就绪
                    sleep(Duration::from_secs(5)).await;
                    // 清空通道里积压的其他事件
                    while let Ok(_) = rx.try_recv() {}

                    log::info!("Detected app folder changes, rebuilding cache...");

                    let new_cache = init_app_cache().await;
                    let mut cache_lock = APP_CACHE.write().await;
                    *cache_lock = Some(new_cache);
                }
            }
        }
    });
}

#[tauri::command]
#[cfg_attr(feature = "specta", specta::specta)]
pub async fn search_apps(query: String) -> Result<Vec<SearchResult>, String> {
    log::info!("search_apps called with query: '{}'", query);
    let apps = get_cached_apps().await;

    let results: Vec<SearchResult> = if query.trim().is_empty() {
        let mut sorted_apps = apps.iter().collect::<Vec<_>>();
        sorted_apps.sort_by(|a, b| b.last_used.cmp(&a.last_used));

        sorted_apps
            .into_iter()
            .take(50)
            .enumerate()
            .map(|(i, app)| SearchResult {
                id: format!("app-{}", i),
                title: app.name.clone(),
                path: app.path.clone(),
                kind: "application".to_string(),
                icon: app.icon_cache.clone(),
                last_used: app.last_used.clone(),
                score: Some((2100 - i as i32).max(1800)),
            })
            .collect()
    } else {
        let results = MATCHER.with(|matcher_cell| {
            let mut matcher = matcher_cell.borrow_mut();
            let pattern = Pattern::parse(&query, CaseMatching::Ignore, Normalization::Smart);
            let mut buf = Vec::new();

            let mut scored_apps: Vec<(u32, &CachedApp)> = apps
                .iter()
                .filter_map(|app| {
                    // 将名称、拼音全拼和拼音首字母用空格组合，以允许混合搜索
                    // 我们使用原始的 `name` 而不是 `name_lower`，以保留 CamelCase 边界以获得 nucleo 加分。
                    let words = word_pinyin_overrides(&app.name);
                    let combined =
                        format!("{} {} {} {} {} {}", app.name, app.bundle_name, app.pinyin_full, app.pinyin_initials, app.pinyin_compact, words);

                    let mut score = pattern
                        .score(
                            nucleo_matcher::Utf32Str::new(&combined, &mut buf),
                            &mut matcher,
                        )
                        .unwrap_or(0);

                    if score > 0 {
                        let system_count = app.use_count.load(Ordering::Relaxed);
                        let session_count = SEARCH_SESSION.session_use_deltas.lock()
                            .ok()
                            .and_then(|d| d.get(&app.path).copied())
                            .unwrap_or(0);
                        let count = system_count + session_count;
                        let boost = std::cmp::min(count * 10, 800);
                        score += boost;
                        Some((score, app))
                    } else {
                        None
                    }
                })
                .collect();

            scored_apps.sort_by(|a, b| b.0.cmp(&a.0));

            scored_apps
                .into_iter()
                .take(20)
                .enumerate()
                .map(|(i, (score, app))| SearchResult {
                    id: format!("app-{}", i),
                    title: app.name.clone(),
                    path: app.path.clone(),
                    kind: "application".to_string(),
                    icon: app.icon_cache.clone(),
                    last_used: app.last_used.clone(),
                    score: Some((score + 2000) as i32),
                })
                .collect()
        });
        results
    };

    Ok(results)
}

#[tauri::command]
#[cfg_attr(feature = "specta", specta::specta)]
pub async fn search_files(query: String) -> Result<Vec<SearchResult>, String> {
    if query.trim().is_empty() {
        return Ok(vec![]);
    }

    let search_id = SEARCH_SESSION.next_search_id();

    let target_dirs = vec![
        "Desktop".to_string(),
        "Documents".to_string(),
        "Downloads".to_string(),
        "Pictures".to_string(),
        "Music".to_string(),
        "Movies".to_string(),
        "Projects".to_string(),
        "Code".to_string(),
    ];

    let mut command = tokio::process::Command::new("mdfind");
    command.arg("-name").arg(&query);
    command.arg("-attr").arg("kMDItemUseCount");

    if let Some(home_dir) = dirs::home_dir() {
        for dir in &target_dirs {
            let path = home_dir.join(dir);
            if path.exists() {
                command.arg("-onlyin").arg(path);
            }
        }
    }

    let mdfind_future = command.output();
    let output = match tokio::time::timeout(std::time::Duration::from_secs(8), mdfind_future).await {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => return Err(format!("mdfind failed: {}", e)),
        Err(_) => {
            log::warn!("mdfind timed out after 8s for query: {}", query);
            return Err("Search timed out".to_string());
        }
    };

    if SEARCH_SESSION.get_current_id() != search_id {
        return Ok(vec![]); // superseded by newer search
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    let entries = parse_mdfind_file_output(&stdout);

    let scored_files: Vec<(u32, u32, String, String)> = MATCHER.with(|matcher_cell| {
        let mut matcher = matcher_cell.borrow_mut();
        let pattern = Pattern::parse(&query, CaseMatching::Ignore, Normalization::Smart);
        let mut buf = Vec::new();

        entries
            .into_iter()
            .take(300)
            .filter_map(|(path, use_count)| {
                let name = Path::new(&path)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Unknown")
                    .to_string();

                let name_pinyin = to_pinyin_full(&name);
                let name_initials = to_pinyin_initials(&name);

                let parent = Path::new(&path)
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                let parent_pinyin = to_pinyin_full(parent);

                let compact = name_pinyin.chars().filter(|c| !c.is_whitespace()).collect::<String>();
                let words = word_pinyin_overrides(&name);
                let combined = format!("{} {} {} {} {} {}", name, name_pinyin, name_initials, parent_pinyin, compact, words);

                let mut score = pattern
                    .score(nucleo_matcher::Utf32Str::new(&combined, &mut buf), &mut matcher)
                    .unwrap_or(0);

                if score > 0 {
                    let boost = std::cmp::min(use_count * 10, 800);
                    score += boost;
                    Some((score, use_count, name, path))
                } else {
                    None
                }
            })
            .collect()
    });

    if SEARCH_SESSION.get_current_id() != search_id {
        return Ok(vec![]);
    }

    let results = tokio::task::spawn_blocking(move || {
        let mut files_with_meta: Vec<(u32, bool, String, String)> = Vec::with_capacity(scored_files.len());

        for (score, _use_count, name, path) in scored_files {
            if SEARCH_SESSION.get_current_id() != search_id {
                return Vec::new();
            }
            let is_dir = std::fs::metadata(&path)
                .map(|m| m.is_dir())
                .unwrap_or(false);
            let final_score = score + if is_dir { 1000 } else { 0 };
            files_with_meta.push((final_score, is_dir, name, path));
        }

        files_with_meta.sort_by(|a, b| b.0.cmp(&a.0));

        files_with_meta
            .into_iter()
            .take(40)
            .enumerate()
            .map(|(i, (final_score, is_dir, name, path))| {
                let kind_str = if is_dir { "folder" } else { "file" };
                SearchResult {
                    id: format!("{}-{}", kind_str, i),
                    title: name,
                    path,
                    kind: kind_str.to_string(),
                    icon: None,
                    last_used: None,
                    score: Some(final_score as i32),
                }
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|e| format!("Failed to compute file metadata: {}", e))?;

    Ok(results)
}

fn parse_mdfind_file_output(stdout: &str) -> Vec<(String, u32)> {
    let mut results = Vec::new();
    let mut current_path = String::new();
    let mut current_use_count: u32 = 0;
    let mut has_pending = false;

    let mut flush = |path: &mut String, use_count: &mut u32, pending: &mut bool| {
        if *pending {
            results.push((std::mem::take(path), std::mem::take(use_count)));
            *pending = false;
        }
    };

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            flush(&mut current_path, &mut current_use_count, &mut has_pending);
            continue;
        }

        if trimmed.starts_with('/') {
            flush(&mut current_path, &mut current_use_count, &mut has_pending);

            let parts: Vec<&str> = trimmed.split("   ").collect();
            current_path = parts.first().map(|s| s.trim().to_string()).unwrap_or_default();

            for part in &parts[1..] {
                if let Some(val) = part.trim().strip_prefix("kMDItemUseCount = ") {
                    current_use_count = val.trim().parse().unwrap_or(0);
                }
            }
            has_pending = true;
        } else if let Some(val) = trimmed.strip_prefix("kMDItemUseCount = ") {
            current_use_count = val.trim().parse().unwrap_or(0);
        }
    }
    flush(&mut current_path, &mut current_use_count, &mut has_pending);

    results
}

#[tauri::command]
pub async fn get_recent_apps() -> Result<Vec<SearchResult>, String> {
    let apps = get_cached_apps().await;

    let results = tokio::task::spawn_blocking(move || {
        let mut res: Vec<SearchResult> = apps
            .iter()
            .map(|app| SearchResult {
                id: format!("recent-{}", app.name.to_lowercase().replace(' ', "-")),
                title: app.name.clone(),
                path: app.path.clone(),
                kind: "application".to_string(),
                icon: app.icon_cache.clone(),
                last_used: app.last_used.clone(),
                score: Some(100),
            })
            .collect();

        // 按最后使用时间排序（降序）
        res.sort_by(|a, b| b.last_used.cmp(&a.last_used));

        res.into_iter().take(10).collect()
    })
    .await
    .map_err(|e| e.to_string())?;

    Ok(results)
}

#[tauri::command]
pub async fn launch_app(path: String) -> Result<(), String> {
    let p = Path::new(&path);
    if !p.is_absolute() {
        return Err(format!("Path is not absolute: {}", path));
    }
    if !p.exists() {
        return Err(format!("Path does not exist: {}", path));
    }

    tokio::process::Command::new("open")
        .arg(&path)
        .spawn()
        .map_err(|e| format!("Failed to launch: {}", e))?;

    SEARCH_SESSION.increment_use_count(&path);

    Ok(())
}

#[tauri::command]
pub async fn reveal_in_finder(path: String) -> Result<(), String> {
    let p = Path::new(&path);
    if !p.is_absolute() {
        return Err(format!("Path is not absolute: {}", path));
    }
    if !p.exists() {
        return Err(format!("Path does not exist: {}", path));
    }

    tokio::process::Command::new("open")
        .arg("-R")
        .arg(&path)
        .spawn()
        .map_err(|e| format!("Failed to reveal: {}", e))?;
    Ok(())
}

#[tauri::command]
#[cfg_attr(feature = "specta", specta::specta)]
pub fn score_items(query: String, items: Vec<String>) -> Vec<u32> {
    if query.trim().is_empty() {
        return vec![0; items.len()];
    }

    MATCHER.with(|matcher_cell| {
        let mut matcher = matcher_cell.borrow_mut();
        let pattern = Pattern::parse(&query, CaseMatching::Ignore, Normalization::Smart);
        let mut buf = Vec::new();

        items.into_iter().map(|item| {
            let pinyin_full = to_pinyin_full(&item);
            let pinyin_initials = to_pinyin_initials(&item);
            let compact = pinyin_full.chars().filter(|c| !c.is_whitespace()).collect::<String>();
            let combined = format!("{} {} {} {}", item, pinyin_full, pinyin_initials, compact);

            pattern.score(
                nucleo_matcher::Utf32Str::new(&combined, &mut buf),
                &mut matcher,
            ).unwrap_or(0)
        }).collect()
    })
}


pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new("search")
        .setup(|app, _api| {
            crate::extensions::search::set_app_handle(app.clone());
            crate::extensions::search::init_app_watcher();
            tauri::async_runtime::spawn(crate::extensions::search::prewarm_cache());
            Ok(())
        })
        .build()
}
