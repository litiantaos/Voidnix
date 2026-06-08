use crate::core::tier1::Tier1Extension;
use std::process::{Command, Stdio, Child};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{State, Emitter, AppHandle};
use tauri::tray::{TrayIconBuilder, MouseButton, TrayIconEvent};

pub struct AwakeState {
    pub process: Mutex<Option<Child>>,
}

static MIRROR_MODE: AtomicBool = AtomicBool::new(true);

const AWAKE_BIN: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/awake_display"));

#[tauri::command]
pub async fn toggle_awake(app: tauri::AppHandle, state: State<'_, AwakeState>, enable: bool) -> Result<bool, String> {
    let mut process_guard = state.process.lock().map_err(|e| e.to_string())?;

    if enable {
        if process_guard.is_some() {
            return Ok(true);
        }

        let temp_dir = std::env::temp_dir().join("com.litiantao.voidnix");
        let _ = std::fs::create_dir_all(&temp_dir);
        let bin_path = temp_dir.join("Display Wakelock");

        std::fs::write(&bin_path, AWAKE_BIN).map_err(|e| e.to_string())?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&bin_path).map_err(|e| e.to_string())?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&bin_path, perms).map_err(|e| e.to_string())?;
        }

        let mode_arg = if MIRROR_MODE.load(Ordering::Relaxed) { "--mirror" } else { "--extend" };
        let child = Command::new(&bin_path)
            .arg(mode_arg)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;

        *process_guard = Some(child);

        if let Some(tray) = app.tray_by_id("awake_tray") {
            let _ = tray.set_visible(true);
        } else {
            let icon_bytes = include_bytes!("../../../public/bar-icon-fill.png");
            let icon = tauri::image::Image::from_bytes(icon_bytes);
            if let Ok(icon) = icon {
                let tray_icon = TrayIconBuilder::with_id("awake_tray")
                    .icon(icon)
                    .icon_as_template(true)
                    .on_tray_icon_event(|tray, event| {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            ..
                        } = event
                        {
                            let app = tray.app_handle().clone();
                            let _ = app.clone().run_on_main_thread(move || {
                                crate::core::window::show_main(&app);
                                let _ = app.emit("open-module", "awake");
                            });
                        }
                    })
                    .build(&app);
                if let Err(e) = tray_icon {
                    eprintln!("Failed to build tray icon: {:?}", e);
                }
            }
        }

        Ok(true)
    } else {
        if let Some(mut child) = process_guard.take() {
            drop(child.stdin.take());
            let _ = child.kill();
            let _ = child.wait();
        }

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

#[tauri::command]
pub async fn set_awake_mode(state: State<'_, AwakeState>, mirror: bool) -> Result<bool, String> {
    MIRROR_MODE.store(mirror, Ordering::Relaxed);

    let mut process_guard = state.process.lock().map_err(|e| e.to_string())?;
    if let Some(mut child) = process_guard.take() {
        drop(child.stdin.take());
        let _ = child.kill();
        let _ = child.wait();

        let mode_arg = if mirror { "--mirror" } else { "--extend" };
        let temp_dir = std::env::temp_dir().join("com.litiantao.voidnix");
        let bin_path = temp_dir.join("Display Wakelock");

        let new_child = Command::new(&bin_path)
            .arg(mode_arg)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;

        *process_guard = Some(new_child);
    }

    Ok(mirror)
}

/// Awake 扩展。
pub struct Plugin;

impl Tier1Extension for Plugin {
    fn id(&self) -> &'static str {
        "awake"
    }

    fn on_setup(&self, app: &AppHandle) -> tauri::Result<()> {
        use tauri::Manager;
        app.manage(AwakeState {
            process: std::sync::Mutex::new(None),
        });
        Ok(())
    }
}
