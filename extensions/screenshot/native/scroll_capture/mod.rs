//! 滚动截屏：状态 / 编码 / 拼接 / 鼠标穿透 / 命令入口。

mod encode;
mod mouse;
mod state;
mod stitch;

use std::process::Command;
use std::sync::atomic::Ordering;

use super::ffi::decode_image_data;
use super::ffi::{
    voidnix_screenshot_clear_background, voidnix_screenshot_get_mouse_location,
    voidnix_screenshot_install_scroll_mask, voidnix_screenshot_remove_scroll_mask,
    voidnix_screenshot_set_ignores_mouse, voidnix_screenshot_set_sharing,
    voidnix_screenshot_window_number,
};

pub use encode::encode_png;
pub use mouse::{start as start_mouse_monitor, stop as stop_mouse_monitor};
pub use state::{ScrollSession, IS_RUNNING, PENDING_TOOLBAR, SESSION};
pub use stitch::capture_loop;

// ── Tauri commands ────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn enter_scroll_capture(
    app: tauri::AppHandle,
    sel_x: f64,
    sel_y: f64,
    sel_w: f64,
    sel_h: f64,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        if IS_RUNNING.load(Ordering::SeqCst) {
            return Err("滚动截屏已在进行中".to_string());
        }

        let (tx, rx) = std::sync::mpsc::channel::<Result<(u32, usize), String>>();
        let app_c = app.clone();
        app.run_on_main_thread(move || {
            use objc2_app_kit::NSWindow;
            use tauri::Manager;
            let r = (|| -> Result<(u32, usize), String> {
                let window = app_c
                    .get_webview_window("screenshot")
                    .ok_or("找不到截图窗口")?;
                let raw = window
                    .ns_window()
                    .map_err(|e| e.to_string())?
                    .cast::<NSWindow>();
                let ptr = raw.cast::<NSWindow>() as *mut std::ffi::c_void;
                let ns_addr = ptr as usize;
                // SAFETY: ptr 来自 window.ns_window() Ok 分支（合法 NSWindow 指针）；
                // voidnix_screenshot_* 为 FFI 薄壳，install_scroll_mask 返回 bool 失败检查，
                // window_number 返回值 >0 校验；均在主线程 run_on_main_thread 闭包内执行
                unsafe {
                    if !voidnix_screenshot_install_scroll_mask(ptr, sel_x, sel_y, sel_w, sel_h) {
                        return Err("装载滚动遮罩失败".to_string());
                    }
                    voidnix_screenshot_set_sharing(ptr, 0);
                    let win_num = voidnix_screenshot_window_number(ptr);
                    if win_num <= 0 {
                        return Err("获取截屏窗口编号失败".to_string());
                    }
                    Ok((win_num as u32, ns_addr))
                }
            })();
            let _ = tx.send(r);
        })
        .map_err(|e| e.to_string())?;
        let (overlay_window_id, ns_window_addr) = rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .map_err(|e| e.to_string())??;

        {
            let mut guard = SESSION.lock().unwrap_or_else(|e| e.into_inner());
            let pending_tb = PENDING_TOOLBAR
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take();
            *guard = Some(ScrollSession {
                sel_x,
                sel_y,
                sel_w,
                sel_h,
                pw: 0,
                ph_per_frame: 0,
                buf: Vec::new(),
                total_rows: 0,
                prev_frame: Vec::new(),
                overlay_window_id,
                ns_window_addr,
                ignoring_mouse: false,
                emit_seq: 0,
                static_mask: Vec::new(),
                static_votes: Vec::new(),
                toolbar_rect: pending_tb,
                static_streak: 0,
            });
        }

        let (tx2, rx2) = std::sync::mpsc::channel::<()>();
        app.run_on_main_thread(move || {
            start_mouse_monitor();
            // SAFETY: get_mouse_location 写入栈 &mut f64；set_ignores_mouse 的 ptr 来自
            // SESSION.ns_window_addr（enter 时由 install_scroll_mask 路径写入合法 NSWindow 地址）；
            // SESSION 锁内只读快照，drop 后重新锁写状态
            unsafe {
                let mut mx: f64 = 0.0;
                let mut my: f64 = 0.0;
                voidnix_screenshot_get_mouse_location(&mut mx, &mut my, 0.0);
                let g = SESSION.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(s) = g.as_ref() {
                    let in_hole = mx >= s.sel_x
                        && mx <= s.sel_x + s.sel_w
                        && my >= s.sel_y
                        && my <= s.sel_y + s.sel_h;
                    if in_hole {
                        let ptr = s.ns_window_addr as *mut std::ffi::c_void;
                        voidnix_screenshot_set_ignores_mouse(ptr, 1);
                        drop(g);
                        let mut g2 = SESSION.lock().unwrap_or_else(|e| e.into_inner());
                        if let Some(s2) = g2.as_mut() {
                            s2.ignoring_mouse = true;
                        }
                    }
                }
            }
            let _ = tx2.send(());
        })
        .map_err(|e| e.to_string())?;
        let _ = rx2.recv();

        IS_RUNNING.store(true, Ordering::SeqCst);

        std::thread::spawn(move || capture_loop(app));
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, sel_x, sel_y, sel_w, sel_h);
        Err("仅支持 macOS".to_string())
    }
}

#[tauri::command]
pub async fn exit_scroll_capture(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        IS_RUNNING.store(false, Ordering::SeqCst);
        std::thread::sleep(std::time::Duration::from_millis(50));
        {
            let mut guard = SESSION.lock().unwrap_or_else(|e| e.into_inner());
            *guard = None;
        }

        let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let app_c = app.clone();
        app.run_on_main_thread(move || {
            use objc2_app_kit::NSWindow;
            use tauri::Manager;
            let r = (|| -> Result<(), String> {
                stop_mouse_monitor();
                let window = app_c
                    .get_webview_window("screenshot")
                    .ok_or("找不到截图窗口")?;
                let raw = window
                    .ns_window()
                    .map_err(|e| e.to_string())?
                    .cast::<NSWindow>();
                let ptr = raw.cast::<NSWindow>() as *mut std::ffi::c_void;
                // SAFETY: ptr 来自 window.ns_window() Ok 分支（合法 NSWindow 指针）；
                // voidnix_screenshot_clear_background/remove_scroll_mask/set_sharing
                // 为 FFI 薄壳（纯副作用，无返回值/资源需调用方释放）；主线程执行
                unsafe {
                    // 先清空背景层内容再恢复可见，避免 remove_scroll_mask 把初始截图
                    // CALayer 重新显示出来，与刚看到的滚动画面跳变造成闪烁。
                    // 同一 main thread tick 内完成，下个渲染帧时 contents 已为 nil。
                    voidnix_screenshot_clear_background(ptr);
                    voidnix_screenshot_remove_scroll_mask(ptr);
                    voidnix_screenshot_set_sharing(ptr, 1);
                }
                Ok(())
            })();
            let _ = tx.send(r);
        })
        .map_err(|e| e.to_string())?;
        rx.recv_timeout(std::time::Duration::from_secs(10))
            .map_err(|e| e.to_string())??;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Ok(())
    }
}

#[tauri::command]
pub async fn set_scroll_toolbar_rect(x: f64, y: f64, w: f64, h: f64) -> Result<(), String> {
    let rect = if w <= 0.0 || h <= 0.0 {
        None
    } else {
        Some((x, y, w, h))
    };
    let mut guard = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(s) = guard.as_mut() {
        s.toolbar_rect = rect;
    } else {
        *PENDING_TOOLBAR.lock().unwrap_or_else(|e| e.into_inner()) = rect;
    }
    Ok(())
}

#[tauri::command]
pub async fn finish_scroll_capture(app: tauri::AppHandle) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        IS_RUNNING.store(false, Ordering::SeqCst);
        std::thread::sleep(std::time::Duration::from_millis(50));

        let session = {
            let mut guard = SESSION.lock().unwrap_or_else(|e| e.into_inner());
            guard.take()
        };
        let session = session.ok_or("无滚动截屏会话".to_string())?;
        if session.total_rows == 0 || session.pw == 0 {
            return Err("未捕获到任何内容".to_string());
        }

        let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let app_c = app.clone();
        app.run_on_main_thread(move || {
            use objc2_app_kit::NSWindow;
            use tauri::Manager;
            let r = (|| -> Result<(), String> {
                stop_mouse_monitor();
                let window = app_c
                    .get_webview_window("screenshot")
                    .ok_or("找不到截图窗口")?;
                let raw = window
                    .ns_window()
                    .map_err(|e| e.to_string())?
                    .cast::<NSWindow>();
                let ptr = raw.cast::<NSWindow>() as *mut std::ffi::c_void;
                // SAFETY: ptr 来自 window.ns_window() Ok 分支（合法 NSWindow 指针）；
                // voidnix_screenshot_clear_background/remove_scroll_mask/set_sharing
                // 为 FFI 薄壳（纯副作用，无返回值/资源需调用方释放）；主线程执行
                unsafe {
                    // 先清空背景层内容再恢复可见，避免 remove_scroll_mask 把初始截图
                    // CALayer 重新显示出来，与刚看到的滚动画面跳变造成闪烁。
                    // 同一 main thread tick 内完成，下个渲染帧时 contents 已为 nil。
                    voidnix_screenshot_clear_background(ptr);
                    voidnix_screenshot_remove_scroll_mask(ptr);
                    voidnix_screenshot_set_sharing(ptr, 1);
                }
                Ok(())
            })();
            let _ = tx.send(r);
        })
        .map_err(|e| e.to_string())?;
        rx.recv_timeout(std::time::Duration::from_secs(10))
            .map_err(|e| e.to_string())??;

        let png = encode_png(&session.buf, session.pw, session.total_rows)?;
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
        Ok(format!("data:image/png;base64,{}", b64))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err("仅支持 macOS".to_string())
    }
}

#[tauri::command]
pub async fn save_scroll_result(result_data_url: String, path: String) -> Result<String, String> {
    let png = decode_image_data(&result_data_url)?;
    let file_path = {
        let p = std::path::Path::new(&path);
        if p.is_dir() {
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            p.join(format!("scroll_screenshot_{}.png", ts))
        } else {
            p.to_path_buf()
        }
    };
    crate::runtime::storage::save_png_safely(&file_path, &png)?;
    Ok(file_path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn copy_scroll_result_to_clipboard(result_data_url: String) -> Result<(), String> {
    let png = decode_image_data(&result_data_url)?;
    #[cfg(target_os = "macos")]
    {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let tmp = std::env::temp_dir().join(format!("voidnix_scroll_{}.png", ts));
        // TempHandle RAII：函数退出（含错误路径）自动清理
        let _tmp_handle = crate::runtime::storage::TempHandle::new(tmp.clone());
        std::fs::write(&tmp, &png).map_err(|e| e.to_string())?;
        let script = format!(
            "set f to POSIX file \"{}\"\nset the clipboard to (read f as «class PNGf»)",
            tmp.display()
        );
        let out = Command::new("osascript")
            .args(["-e", &script])
            .output()
            .map_err(|e| e.to_string())?;
        // tmp 由 _tmp_handle Drop 清理
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = png;
        Err("仅支持 macOS".to_string())
    }
}
