use std::process::{Command, Stdio, Child};
use std::sync::Mutex;
use tauri::{State, Manager, Emitter};
use tauri::tray::{TrayIconBuilder, MouseButton, TrayIconEvent};

pub struct AwakeState {
    pub process: Mutex<Option<Child>>,
}

// Embed the compiled executable
const AWAKE_BIN: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/awake_display"));

#[tauri::command]
pub async fn toggle_awake(app: tauri::AppHandle, state: State<'_, AwakeState>, enable: bool) -> Result<bool, String> {
    let mut process_guard = state.process.lock().map_err(|e| e.to_string())?;

    if enable {
        if process_guard.is_some() {
            return Ok(true); // Already running
        }

        // Write the executable to a temporary file
        let temp_dir = std::env::temp_dir().join("com.litiantao.voidnix");
        let _ = std::fs::create_dir_all(&temp_dir);
        let bin_path = temp_dir.join("Display Wakelock");
        
        std::fs::write(&bin_path, AWAKE_BIN).map_err(|e| e.to_string())?;
        
        // Ensure it is executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&bin_path).map_err(|e| e.to_string())?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&bin_path, perms).map_err(|e| e.to_string())?;
        }

        // Spawn the process
        let child = Command::new(&bin_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;

        *process_guard = Some(child);
        
        // Setup tray icon
        if let Some(tray) = app.tray_by_id("awake_tray") {
            let _ = tray.set_visible(true);
        } else {
            // Load the converted PNG icon using Tauri v2 Image API
            let icon_bytes = include_bytes!("../../../public/bar-icon-fill.png");
            
            // In Tauri v2, if "image-png" feature is enabled, Image::from_bytes is available
            let icon = tauri::image::Image::from_bytes(icon_bytes);
            if let Ok(icon) = icon {
                let _ = TrayIconBuilder::with_id("awake_tray")
                    .icon(icon)
                    .icon_as_template(true)
                    .on_tray_icon_event(|tray, event| {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            ..
                        } = event
                        {
                            let app = tray.app_handle();
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                                let _ = app.emit("open-module", "awake");
                            }
                        }
                    })
                    .build(&app);
            }
        }
        
        Ok(true)
    } else {
        if let Some(mut child) = process_guard.take() {
            // Close stdin, which causes the process to exit
            drop(child.stdin.take());
            // Optionally wait or kill
            let _ = child.kill();
            let _ = child.wait();
        }
        
        // Hide tray icon instead of removing it to prevent macOS crash
        if let Some(tray) = app.tray_by_id("awake_tray") {
            let _ = tray.set_visible(false);
        }
        
        Ok(false)
    }
}

#[tauri::command]
pub async fn is_awake_enabled(state: State<'_, AwakeState>) -> Result<bool, String> {
    let process_guard = state.process.lock().map_err(|e| e.to_string())?;
    Ok(process_guard.is_some())
}


pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new("awake")
        .setup(|app, _api| {
            use tauri::Manager;
            app.manage(AwakeState {
                process: std::sync::Mutex::new(None),
            });
            Ok(())
        })
        .build()
}
