//! 访达快捷操作：全局快捷键触发，经 JXA 读取选区/当前目录。
//! 不依赖 FinderSync .appex。
//!
//! - 新建文件：前端先命名，Rust 直接用最终名创建并 reveal（不模拟重命名键）
//! - 隐藏文件：System Events 发 Cmd+Shift+. 切换（系统不暴露稳定可读状态，UI 不做态区分）

use crate::runtime::registry::Extension;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

use serde::Deserialize;
use tauri::AppHandle;

/// Finder 扩展。
pub struct FinderExtExtension;

#[async_trait::async_trait]
impl Extension for FinderExtExtension {
    fn id(&self) -> &'static str {
        "finder-ext"
    }
}

// ── 公共命令 ───────────────────────────────────────────────────────────────

/// 执行访达动作。
/// `name` 仅 `new_file` 使用（最终文件名，已含扩展名）；`app_path` 仅 `open_with` 使用（.app 路径）。
#[tauri::command]
pub async fn finder_run_action(
    app: AppHandle,
    action: String,
    name: Option<String>,
    app_path: Option<String>,
) -> Result<String, String> {
    // osascript / 文件 IO / sleep 阻塞调用放 blocking 线程，避免卡 main thread
    tokio::task::spawn_blocking(move || {
        run_action_inner(&app, &action, name.as_deref(), app_path.as_deref())
    })
    .await
    .map_err(|e| e.to_string())?
}

fn run_action_inner(
    app: &AppHandle,
    action: &str,
    name: Option<&str>,
    app_path: Option<&str>,
) -> Result<String, String> {
    match action {
        "copy_path" => {
            require_finder_frontmost()?;
            let ctx = finder_context()?;
            handle_copy_path(&ctx.paths, &ctx.target)?;
            Ok("已复制路径".into())
        }
        "open_terminal" => {
            require_finder_frontmost()?;
            let ctx = finder_context()?;
            handle_open_terminal(&ctx.paths, &ctx.target)?;
            Ok(String::new())
        }
        "open_with" => {
            require_finder_frontmost()?;
            let ctx = finder_context()?;
            let app_path = app_path.ok_or_else(|| "缺少应用路径".to_string())?;
            handle_open_with(&ctx.paths, &ctx.target, app_path)?;
            Ok(String::new())
        }
        "new_file" => {
            require_finder_frontmost()?;
            let ctx = finder_context()?;
            let file_name = name.ok_or_else(|| "缺少文件名".to_string())?;
            let path = create_named_file(&ctx.target, file_name)?;
            let _ = hide_main_sync(app);
            reveal_in_finder(&path);
            Ok(String::new())
        }
        "toggle_hidden" => {
            // 权限检查须在 hide 前（失败 toast 可见）；注入在 hide 后（见 handle_toggle_hidden）
            handle_toggle_hidden(app)?;
            Ok(String::new())
        }
        other => Err(format!("未知动作: {other}")),
    }
}

/// 返回访达当前选中的文件路径（仅前台为访达时）。
/// 供前端按文件类型做上下文入口（如选中视频时显示「视频处理」）。
#[tauri::command]
pub async fn finder_selected_paths() -> Result<Vec<String>, String> {
    // osascript 阻塞调用放 blocking 线程，避免卡 main thread（进入扩展时 UI 冻结）
    tokio::task::spawn_blocking(selected_paths_inner)
        .await
        .map_err(|e| e.to_string())?
}

fn selected_paths_inner() -> Result<Vec<String>, String> {
    require_finder_frontmost()?;
    let ctx = finder_context()?;
    let paths = ctx
        .paths
        .into_iter()
        .filter(|p| validate_path(Path::new(p)))
        .collect();
    Ok(paths)
}

/// 返回 LaunchServices 注册意义上能打开指定路径的应用（.app 路径，偏好序：默认应用在前）。
/// 供「用 App 打开」做类型推荐候选（同访达「打开方式」数据源，经 NSWorkspace 现代封装）；
/// Finder 自身是目录的默认 handler，用 Finder 打开无价值，过滤。
#[tauri::command]
pub async fn finder_open_with_apps(path: String) -> Result<Vec<String>, String> {
    // LS 查询含磁盘 IO，放 blocking 线程（NSWorkspace 查询与 finder_pid 同款先例）
    tokio::task::spawn_blocking(move || open_with_apps_inner(&path))
        .await
        .map_err(|e| e.to_string())?
}

fn open_with_apps_inner(path: &str) -> Result<Vec<String>, String> {
    // 入参来自 finder_selected_paths（已经 path_guard 校验）；此处仅存在性防抖，
    // 不走 path_guard——/System 下选中的文件同样有推荐应用
    let p = PathBuf::from(path);
    if !p.is_absolute() || !p.exists() {
        return Ok(vec![]);
    }
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::NSWorkspace;
        use objc2_foundation::{NSString, NSURL};
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        let apps = NSWorkspace::sharedWorkspace().URLsForApplicationsToOpenURL(&url);
        Ok(apps
            .iter()
            .filter_map(|u| u.path())
            .map(|p| normalize_ls_path(&p.to_string()))
            .filter(|s| !s.is_empty() && !is_finder_app(s))
            .collect())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(vec![])
    }
}

/// Cryptexes 投影路径归一化：LS 对系统应用（cryptex 卷内容）返回
/// `/System/Volumes/Preboot/Cryptexes/App/System/...` 投影路径，与应用枚举
/// （search_apps 的 `/System/...`）不一致，直接返回会致前端 join 失败、
/// 系统应用候选（TextEdit/Preview 等）被静默剔除。剥前缀还原常规路径；
/// 还原后不存在（非 System firmlink 内容）则保留原路径。
fn normalize_ls_path(path: &str) -> String {
    const CRYPTEX_PREFIX: &str = "/System/Volumes/Preboot/Cryptexes/App";
    if let Some(rest) = path.strip_prefix(CRYPTEX_PREFIX) {
        if Path::new(rest).exists() {
            return rest.to_string();
        }
    }
    path.to_string()
}

fn is_finder_app(path: &str) -> bool {
    path == "/System/Library/CoreServices/Finder.app"
}

// ── Finder 上下文 ──────────────────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
struct FinderContext {
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    target: String,
}

fn is_finder_frontmost() -> bool {
    #[cfg(target_os = "macos")]
    {
        let ws = objc2_app_kit::NSWorkspace::sharedWorkspace();
        ws.frontmostApplication()
            .and_then(|a| a.bundleIdentifier())
            .map(|b| b.to_string() == "com.apple.finder")
            .unwrap_or(false)
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

fn require_finder_frontmost() -> Result<(), String> {
    if is_finder_frontmost() {
        Ok(())
    } else {
        Err("请先切换到访达".into())
    }
}

fn finder_context() -> Result<FinderContext, String> {
    const SCRIPT: &str = r#"
ObjC.import('Foundation');
function run() {
  var finder = Application('Finder');
  var paths = [];
  try {
    var sel = finder.selection();
    for (var i = 0; i < sel.length; i++) {
      var u = sel[i].url();
      if (u) {
        var p = $.NSURL.URLWithString(u).path;
        if (p) paths.push(ObjC.unwrap(p));
      }
    }
  } catch (e) {}
  var target = '';
  try {
    if (finder.finderWindows.length > 0) {
      var t = finder.finderWindows[0].target();
      var tu = t.url();
      if (tu) {
        var tp = $.NSURL.URLWithString(tu).path;
        if (tp) target = ObjC.unwrap(tp);
      }
    }
  } catch (e) {}
  return JSON.stringify({ paths: paths, target: target });
}
"#;

    let output = Command::new("osascript")
        .args(["-l", "JavaScript", "-e", SCRIPT])
        .output()
        .map_err(|e| format!("osascript 调用失败: {e}"))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        if err.contains("not allowed") || err.contains("(-1743)") || err.contains("1002") {
            return Err("需要授权 Voidnix 控制访达（系统设置 → 隐私与安全性 → 自动化）".into());
        }
        let msg = err.trim();
        return Err(format!(
            "读取访达上下文失败: {}",
            if msg.is_empty() { "未知错误" } else { msg }
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let raw = stdout.trim();
    if raw.is_empty() {
        return Ok(FinderContext::default());
    }

    serde_json::from_str(raw).map_err(|e| format!("解析访达上下文失败: {e}"))
}

// ── 窗口 / 权限 ────────────────────────────────────────────────────────────

fn hide_main_sync(app: &AppHandle) -> Result<(), String> {
    let (tx, rx) = mpsc::channel();
    let app2 = app.clone();
    app.run_on_main_thread(move || {
        crate::runtime::window::hide_main(&app2);
        let _ = tx.send(());
    })
    .map_err(|e| e.to_string())?;
    rx.recv_timeout(Duration::from_secs(2))
        .map_err(|_| "隐藏主窗口超时".to_string())?;
    Ok(())
}

/// 辅助功能未授权时弹系统提示并打开设置（授权会话）。
fn ensure_accessibility(app: &AppHandle) -> Result<(), String> {
    if crate::platform::permission::check_accessibility() {
        return Ok(());
    }
    // 弹出系统授权对话框（若从未提示过）
    let _ = crate::platform::permission::request_accessibility();
    if crate::platform::permission::check_accessibility() {
        return Ok(());
    }
    crate::platform::permission::open_privacy_settings(app, "accessibility");
    Err("切换隐藏文件需要辅助功能权限：系统设置 → 隐私与安全性 → 辅助功能 → 启用 Voidnix".into())
}

// ── 动作实现 ───────────────────────────────────────────────────────────────

fn validate_path(path: &Path) -> bool {
    crate::platform::path_guard::validate(path)
}

fn handle_copy_path(paths: &[String], target: &str) -> Result<(), String> {
    let lines: Vec<String> = if !paths.is_empty() {
        paths
            .iter()
            .map(PathBuf::from)
            .filter(|p| validate_path(p))
            .map(|p| p.to_string_lossy().to_string())
            .collect()
    } else if !target.is_empty() {
        let p = PathBuf::from(target);
        if !validate_path(&p) {
            return Err("路径不安全".into());
        }
        vec![target.to_string()]
    } else {
        return Err("未选中任何项目".into());
    };

    if lines.is_empty() {
        return Err("未选中任何项目".into());
    }
    crate::platform::pasteboard::write_text(&lines.join("\n"));
    Ok(())
}

fn handle_open_terminal(paths: &[String], target: &str) -> Result<(), String> {
    let dir = if let Some(raw) = paths.first() {
        let p = PathBuf::from(raw);
        if !validate_path(&p) {
            return Err("路径不安全".into());
        }
        if p.is_dir() {
            p
        } else {
            p.parent()
                .map(|x| x.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."))
        }
    } else if !target.is_empty() {
        let pb = Path::new(target);
        if !validate_path(pb) {
            return Err("路径不安全".into());
        }
        let resolved = pb.canonicalize().unwrap_or_else(|_| pb.to_path_buf());
        if resolved.is_dir() {
            resolved
        } else {
            resolved
                .parent()
                .map(|x| x.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."))
        }
    } else {
        return Err("无可用目录（请打开访达窗口或选中项目）".into());
    };

    Command::new("open")
        .args(["-b", "com.apple.Terminal", dir.to_string_lossy().as_ref()])
        .status()
        .map_err(|e| format!("打开终端失败: {e}"))?;
    Ok(())
}

/// open_with 应用路径专用校验：绝对路径 + .app 后缀 + 存在。
/// 不经 path_guard——系统内置应用在 /System/Applications（path_guard 拦 /System 前缀），
/// 启动交 LaunchServices（`open -a`）无文件系统写，且路径源自应用枚举缓存而非用户输入。
fn validate_app_path(raw: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(raw);
    if !p.is_absolute() {
        return Err("应用路径须为绝对路径".into());
    }
    if p.extension().and_then(|e| e.to_str()) != Some("app") {
        return Err("不是有效的应用".into());
    }
    if !p.is_dir() {
        return Err("应用不存在或已卸载".into());
    }
    Ok(p)
}

/// 用指定应用打开访达选中项（多选全部打开，语义同访达「打开方式」多选）；
/// 无选中回退当前窗口目标目录（与 copy_path / open_terminal 一致）。
fn handle_open_with(paths: &[String], target: &str, app_path: &str) -> Result<(), String> {
    let app = validate_app_path(app_path)?;

    let targets: Vec<String> = if !paths.is_empty() {
        let valid: Vec<String> = paths
            .iter()
            .filter(|p| validate_path(Path::new(p.as_str())))
            .cloned()
            .collect();
        if valid.is_empty() {
            return Err("未选中任何项目".into());
        }
        valid
    } else if !target.is_empty() {
        if !validate_path(Path::new(target)) {
            return Err("路径不安全".into());
        }
        vec![target.to_string()]
    } else {
        return Err("无可用目录（请打开访达窗口或选中项目）".into());
    };

    let mut cmd = Command::new("open");
    cmd.arg("-a").arg(&app);
    for t in &targets {
        cmd.arg(t);
    }
    let status = cmd.status().map_err(|e| format!("打开失败: {e}"))?;
    if !status.success() {
        return Err("打开失败".into());
    }
    Ok(())
}

/// 访达进程 PID（`com.apple.finder`）。
fn finder_pid() -> Option<i32> {
    #[cfg(target_os = "macos")]
    {
        let ws = objc2_app_kit::NSWorkspace::sharedWorkspace();
        ws.runningApplications().iter().find_map(|a| {
            let is_finder = a
                .bundleIdentifier()
                .map(|b| b.to_string() == "com.apple.finder")
                .unwrap_or(false);
            if is_finder {
                Some(a.processIdentifier())
            } else {
                None
            }
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// 切换隐藏文件：发 Cmd+Shift+.
///
/// 时序关键（否则常需点两次才生效）：
/// 1. 权限检查（窗口仍可见，失败 toast 可看）
/// 2. **先 hide 主窗** 并 `restore_captured`，把 key 交还系统
/// 3. 强制前置访达，短等焦点稳定
/// 4. `post_combo` 注入到访达 PID（与 clipboard 等同路径；比 System Events 稳）
///
/// 系统不提供稳定可读显示态，此处只做切换。
fn handle_toggle_hidden(app: &AppHandle) -> Result<(), String> {
    ensure_accessibility(app)?;

    let pid = finder_pid().ok_or_else(|| "未找到访达进程".to_string())?;

    // 面板仍 key 时注入常被吞掉或落到错误进程 → 表现为「要切两次」
    let _ = hide_main_sync(app);

    #[cfg(target_os = "macos")]
    {
        // hide 已 restore 原前台；原前台未必是访达（从其它 app 进模块），强制前置
        crate::platform::focus::activate_app_by_pid(pid);

        // 等 activate / key window 落稳（过短则第一次 key 丢失）
        for _ in 0..8 {
            if is_finder_frontmost() {
                break;
            }
            std::thread::sleep(Duration::from_millis(40));
            crate::platform::focus::activate_app_by_pid(pid);
        }
        std::thread::sleep(Duration::from_millis(80));

        crate::platform::input::post_combo("cmd+shift+.", Some(pid));
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = pid;
        Err("仅支持 macOS".into())
    }
}

/// 校验并清洗文件名（单层名，禁止路径分隔）。
fn sanitize_filename(raw: &str) -> Result<String, String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err("文件名不能为空".into());
    }
    if name == "." || name == ".." {
        return Err("非法文件名".into());
    }
    if name.contains('/') || name.contains('\0') {
        return Err("文件名不能包含路径分隔符".into());
    }
    // macOS 允许冒号在 POSIX 层，但 Finder 显示易混，直接拒
    if name.contains(':') {
        return Err("文件名不能包含冒号".into());
    }
    if name.len() > 255 {
        return Err("文件名过长".into());
    }
    Ok(name.to_string())
}

fn create_named_file(target: &str, file_name: &str) -> Result<PathBuf, String> {
    if target.is_empty() {
        return Err("无可用目录（请打开访达窗口）".into());
    }
    let pb = Path::new(target);
    if !validate_path(pb) {
        return Err("路径不安全".into());
    }
    let dir = pb.canonicalize().unwrap_or_else(|_| pb.to_path_buf());
    if !dir.is_dir() {
        return Err("当前目标不是文件夹".into());
    }

    let base = sanitize_filename(file_name)?;
    // 冲突时 Untitled.txt → Untitled 2.txt
    let (stem, ext) = split_stem_ext(&base);
    let mut counter: u32 = 0;
    loop {
        counter += 1;
        if counter > 10_000 {
            return Err("新建文件失败：命名冲突过多".into());
        }
        let filename = if counter == 1 {
            base.clone()
        } else if ext.is_empty() {
            format!("{stem} {counter}")
        } else {
            format!("{stem} {counter}.{ext}")
        };
        let path = dir.join(&filename);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => return Ok(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("创建文件失败: {e}")),
        }
    }
}

fn split_stem_ext(name: &str) -> (String, String) {
    if let Some((s, e)) = name.rsplit_once('.') {
        if !s.is_empty() && !e.is_empty() && !e.contains(' ') {
            return (s.to_string(), e.to_string());
        }
    }
    (name.to_string(), String::new())
}

fn reveal_in_finder(path: &Path) {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::NSWorkspace;
        let path_str = path.to_string_lossy().to_string();
        let ns_path = objc2_foundation::NSString::from_str(&path_str);
        let ws = NSWorkspace::sharedWorkspace();
        let _ = ws.selectFile_inFileViewerRootedAtPath(
            Some(&ns_path),
            &objc2_foundation::NSString::from_str(""),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_with_apps_nonexistent_path_returns_empty() {
        assert!(open_with_apps_inner("/definitely/not/here/voidnix")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn finder_app_filter() {
        assert!(is_finder_app("/System/Library/CoreServices/Finder.app"));
        assert!(!is_finder_app("/Applications/Visual Studio Code.app"));
    }

    #[test]
    fn ls_path_normalization() {
        // Cryptexes 投影路径还原常规路径（System firmlink 内容存在）
        let sys = "/System/Applications";
        if Path::new(sys).is_dir() {
            assert_eq!(
                normalize_ls_path(
                    "/System/Volumes/Preboot/Cryptexes/App/System/Applications/Notes.app"
                ),
                "/System/Applications/Notes.app"
            );
        }
        // 非 Cryptexes 前缀原样返回；还原后不存在也保留原路径
        assert_eq!(
            normalize_ls_path("/Applications/Visual Studio Code.app"),
            "/Applications/Visual Studio Code.app"
        );
        assert_eq!(
            normalize_ls_path("/System/Volumes/Preboot/Cryptexes/App/Not/Here.app"),
            "/System/Volumes/Preboot/Cryptexes/App/Not/Here.app"
        );
    }

    #[test]
    fn app_path_validation() {
        // 相对路径拒绝
        assert!(validate_app_path("Applications/X.app").is_err());
        // 非 .app 后缀拒绝
        assert!(validate_app_path("/Applications/readme.txt").is_err());
        // 不存在的 .app 拒绝
        assert!(validate_app_path("/Applications/VoidnixNotHere9999.app").is_err());
        // 存在的 .app 目录放行
        let dir = std::env::temp_dir().join("voidnix-fw-app-validate.app");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(validate_app_path(dir.to_str().unwrap()).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
        // 系统内置应用放行（/System 前缀不经 path_guard，见函数注释）
        let sys = Path::new("/System/Applications");
        if sys.is_dir() {
            let sample = std::fs::read_dir(sys)
                .unwrap()
                .flatten()
                .map(|e| e.path())
                .find(|p| p.extension().is_some_and(|e| e == "app"))
                .expect("系统应用目录应含 .app");
            assert!(validate_app_path(sample.to_str().unwrap()).is_ok());
        }
    }
}
