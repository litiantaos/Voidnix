use base64::Engine;
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Serialize, Deserialize, Clone)]
pub struct ScreenshotData {
    pub data_url: String,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

/// 截取主屏幕，返回 base64 PNG data URL
#[tauri::command]
pub fn capture_screen() -> Result<ScreenshotData, String> {
    use core_graphics::display::CGDisplay;

    let display = CGDisplay::main();
    let bounds = display.bounds();
    let lw = bounds.size.width as u32;
    let lh = bounds.size.height as u32;

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let tmp = std::env::temp_dir().join(format!("voidnix_cap_{}.png", ts));

    // screencapture -x: 静音
    let out = std::process::Command::new("screencapture")
        .args(["-x", tmp.to_str().unwrap()])
        .output()
        .map_err(|e| e.to_string())?;

    if !out.status.success() {
        return Err("截图失败：请在「系统设置 → 隐私与安全性 → 屏幕录制」中授权".to_string());
    }

    let data = std::fs::read(&tmp).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&tmp);

    let pw = if data.len() >= 24 {
        u32::from_be_bytes([data[16], data[17], data[18], data[19]])
    } else { lw };
    let scale = if lw > 0 { pw as f64 / lw as f64 } else { 1.0 };

    let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
    Ok(ScreenshotData {
        data_url: format!("data:image/png;base64,{}", b64),
        width: lw,
        height: lh,
        scale,
    })
}

/// OCR 识别图像文字（Apple Vision，无需权限）
#[tauri::command]
pub async fn ocr_image(image_data: String) -> Result<String, String> {
    let data = decode_image_data(&image_data)?;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let tmp = std::env::temp_dir().join(format!("voidnix_ocr_{}.png", ts));
    std::fs::write(&tmp, &data).map_err(|e| e.to_string())?;

    let script = format!(
        r#"import Vision; import AppKit
let url = URL(fileURLWithPath: "{path}")
guard let img = NSImage(contentsOf: url), let cg = img.cgImage(forProposedRect: nil, context: nil, hints: nil) else {{ print(""); exit(0) }}
let req = VNRecognizeTextRequest()
req.recognitionLevel = .accurate; req.usesLanguageCorrection = true
req.recognitionLanguages = ["zh-Hans","zh-Hant","en-US","ja"]
try? VNImageRequestHandler(cgImage: cg, options: [:]).perform([req])
print((req.results ?? []).compactMap {{ $0.topCandidates(1).first?.string }}.joined(separator: "\n"))"#,
        path = tmp.display()
    );

    let out = Command::new("swift").args(["-e", &script]).output()
        .map_err(|e| format!("swift 失败: {}", e))?;
    let _ = std::fs::remove_file(&tmp);

    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// 保存图像到指定路径
#[tauri::command]
pub async fn save_screenshot(image_data: String, path: String) -> Result<(), String> {
    std::fs::write(&path, decode_image_data(&image_data)?).map_err(|e| e.to_string())
}

/// 复制图像到系统剪贴板（用 osascript 避免 objc2 API 兼容问题）
#[tauri::command]
pub async fn copy_screenshot_to_clipboard(image_data: String) -> Result<(), String> {
    let data = decode_image_data(&image_data)?;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let tmp = std::env::temp_dir().join(format!("voidnix_clip_{}.png", ts));
    std::fs::write(&tmp, &data).map_err(|e| e.to_string())?;

    // 用 osascript 将文件写入剪贴板
    let script = format!(
        r#"set f to POSIX file "{}"
set the clipboard to (read f as «class PNGf»)"#,
        tmp.display()
    );
    let out = Command::new("osascript").args(["-e", &script]).output()
        .map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&tmp);

    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

fn decode_image_data(s: &str) -> Result<Vec<u8>, String> {
    let b64 = if let Some(p) = s.find(',') { &s[p + 1..] } else { s };
    base64::engine::general_purpose::STANDARD.decode(b64).map_err(|e| e.to_string())
}

/// 显示截图覆盖层窗口（预置为全屏），emit 截图数据
#[tauri::command]
pub async fn enter_screenshot_mode(app: tauri::AppHandle, data: ScreenshotData) -> Result<(), String> {
    let app_clone = app.clone();
    let data_clone = data.clone();
    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
    app.run_on_main_thread(move || {
        let _ = tx.send(enter_impl(&app_clone, &data_clone));
    }).map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())?
}

#[cfg(target_os = "macos")]
fn enter_impl(app: &tauri::AppHandle, data: &ScreenshotData) -> Result<(), String> {
    use tauri::Manager;
    use objc2_app_kit::{NSScreen, NSWindow, NSWindowAnimationBehavior};
    use objc2_foundation::MainThreadMarker;

    // 隐藏主窗口并移除点击监视器
    if let Some(main_window) = app.get_webview_window("main") {
        let _ = main_window.hide();
    }
    crate::commands::shortcut::remove_click_monitor();

    let window = app.get_webview_window("screenshot").ok_or("找不到截图窗口")?;
    let raw = window.ns_window().map_err(|e| e.to_string())?.cast::<NSWindow>();
    unsafe {
        let ns_window: &NSWindow = raw.as_ref().ok_or("NSWindow 为空")?;

        let mtm = MainThreadMarker::new().ok_or("不在主线程")?;
        if let Some(screen) = NSScreen::mainScreen(mtm) {
            ns_window.setFrame_display(screen.frame(), true);
        }

        ns_window.setAnimationBehavior(NSWindowAnimationBehavior::None);

        if let Ok(data_json) = serde_json::to_string(data) {
            let _ = window.eval(&format!(
                "window.__screenshotData = {}; window.dispatchEvent(new CustomEvent('__screenshot_ready'));",
                data_json
            ));
        }

        let _: () = objc2::msg_send![ns_window, setIgnoresMouseEvents: false];
        ns_window.setAlphaValue(1.0);
        crate::mac_utils::activate_app();
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn enter_impl(_app: &tauri::AppHandle, _data: &ScreenshotData) -> Result<(), String> {
    Err("仅支持 macOS".to_string())
}

/// 退出截屏模式：隐藏截图覆盖层窗口（保持 JS 运行）
#[tauri::command]
pub async fn exit_screenshot_mode(app: tauri::AppHandle) -> Result<(), String> {
    let app_clone = app.clone();
    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
    app.run_on_main_thread(move || {
        let _ = tx.send(exit_impl(&app_clone));
    }).map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())?
}

#[cfg(target_os = "macos")]
fn exit_impl(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    use objc2_app_kit::{NSWindow, NSWindowAnimationBehavior};

    let window = app.get_webview_window("screenshot").ok_or("找不到截图窗口")?;
    let raw = window.ns_window().map_err(|e| e.to_string())?.cast::<NSWindow>();
    unsafe {
        let ns_window: &NSWindow = raw.as_ref().ok_or("NSWindow 为空")?;
        ns_window.setAnimationBehavior(NSWindowAnimationBehavior::None);
        // 不 orderOut——保持 JS 运行；只隐藏并禁止鼠标事件
        ns_window.setAlphaValue(0.0);
        let _: () = objc2::msg_send![ns_window, setIgnoresMouseEvents: true];
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn exit_impl(_app: &tauri::AppHandle) -> Result<(), String> {
    Ok(())
}
