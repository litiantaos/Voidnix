use crate::runtime::registry::Extension;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{State, Emitter, AppHandle};
use tauri::tray::{TrayIconBuilder, MouseButton, TrayIconEvent};

/// 托管 awake 子进程：Drop 时自动 kill+wait，覆盖 app 正常退出/状态释放场景。
/// panic=abort 下 Drop 不跑，由 awake binary 检测 stdin 关闭自行退出兜底。
pub(crate) struct ManagedChild(Option<Child>);
impl Drop for ManagedChild {
    fn drop(&mut self) {
        if let Some(mut c) = self.0.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

pub struct AwakeState {
    pub process: Mutex<Option<ManagedChild>>,
}

static MIRROR_MODE: AtomicBool = AtomicBool::new(true);

const AWAKE_BIN: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/awake_display"));

/// 返回 awake binary 存放路径（app_data_dir/extensions/awake/，非 /tmp）。
fn awake_bin_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    use tauri::Manager;
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let ext_dir = data_dir.join("extensions").join("awake");
    std::fs::create_dir_all(&ext_dir).map_err(|e| e.to_string())?;
    Ok(ext_dir.join("Display Wakelock"))
}

#[tauri::command]
pub async fn toggle_awake(app: tauri::AppHandle, state: State<'_, AwakeState>, enable: bool) -> Result<bool, String> {
    let mut process_guard = state.process.lock().map_err(|e| e.to_string())?;

    if enable {
        if process_guard.is_some() {
            return Ok(true);
        }

        let bin_path = awake_bin_path(&app)?;

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

        *process_guard = Some(ManagedChild(Some(child)));
        drop(process_guard); // 释放锁，避免 MutexGuard 存活到函数末尾影响 async Send 判定

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
                                crate::runtime::window::show_main(&app);
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
        // take 出 child 后立即 drop guard，避免 std::sync::MutexGuard 跨 await 点（非 Send）
        let child_opt = process_guard.take();
        drop(process_guard);
        if let Some(mut managed) = child_opt {
            if let Some(mut child) = managed.0.take() {
                drop(child.stdin.take());
                let _ = child.kill();
                let _ = child.wait();
            }
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
pub async fn set_awake_mode(app: tauri::AppHandle, state: State<'_, AwakeState>, mirror: bool) -> Result<bool, String> {
    MIRROR_MODE.store(mirror, Ordering::Relaxed);

    let mut process_guard = state.process.lock().map_err(|e| e.to_string())?;
    if let Some(mut managed) = process_guard.take() {
        // drop guard 避免跨 await；重新 spawn 后再 lock 存入
        drop(process_guard);
        if let Some(mut child) = managed.0.take() {
            drop(child.stdin.take());
            let _ = child.kill();
            let _ = child.wait();
        }

        let mode_arg = if mirror { "--mirror" } else { "--extend" };
        let bin_path = awake_bin_path(&app)?;

        let new_child = Command::new(&bin_path)
            .arg(mode_arg)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;

        let mut guard = state.process.lock().map_err(|e| e.to_string())?;
        *guard = Some(ManagedChild(Some(new_child)));
    }

    Ok(mirror)
}

/// 命令注册（局部 invoke_handler，§2.8）。
pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new("awake")
        .build()
}

/// Awake 扩展。
pub struct AwakeExtension;

#[async_trait::async_trait]
impl Extension for AwakeExtension {
    fn id(&self) -> &'static str {
        "awake"
    }

    async fn setup(&self, app: &AppHandle) -> tauri::Result<()> {
        use tauri::Manager;
        app.manage(AwakeState {
            process: std::sync::Mutex::new(None),
        });
        Ok(())
    }
}
