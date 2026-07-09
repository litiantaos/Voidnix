use crate::runtime::menubar::{MenuBarContribution, MenuEntry};
use crate::runtime::registry::Extension;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

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
    let dir = crate::runtime::storage::ext_data_dir(app, "awake")?;
    Ok(dir.join("Display Wakelock"))
}

/// 停止 awake 子进程（take + kill + wait）。
fn stop_awake(state: &AwakeState) {
    let managed_opt = crate::runtime::lock_or_recover(&state.process).take();
    if let Some(mut managed) = managed_opt {
        if let Some(mut child) = managed.0.take() {
            drop(child.stdin.take());
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// 以指定模式重启 awake 子进程（未运行则跳过）。
fn restart_awake(app: &AppHandle, state: &AwakeState, mirror: bool) -> Result<(), String> {
    let managed_opt = crate::runtime::lock_or_recover(&state.process).take();
    let Some(mut managed) = managed_opt else {
        return Ok(());
    };
    if let Some(mut child) = managed.0.take() {
        drop(child.stdin.take());
        let _ = child.kill();
        let _ = child.wait();
    }
    let mode_arg = if mirror { "--mirror" } else { "--extend" };
    let bin_path = awake_bin_path(app)?;
    let new_child = Command::new(&bin_path)
        .arg(mode_arg)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    *crate::runtime::lock_or_recover(&state.process) = Some(ManagedChild(Some(new_child)));
    Ok(())
}

#[tauri::command]
pub async fn set_awake_enabled(
    app: AppHandle,
    state: State<'_, AwakeState>,
    enabled: bool,
) -> Result<bool, String> {
    if enabled {
        {
            let process_guard = state.process.lock().map_err(|e| e.to_string())?;
            if process_guard.is_some() {
                return Ok(true);
            }
        }

        let bin_path = awake_bin_path(&app)?;

        std::fs::write(&bin_path, AWAKE_BIN).map_err(|e| e.to_string())?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&bin_path)
                .map_err(|e| e.to_string())?
                .permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&bin_path, perms).map_err(|e| e.to_string())?;
        }

        let mode_arg = if MIRROR_MODE.load(Ordering::Relaxed) {
            "--mirror"
        } else {
            "--extend"
        };
        let child = Command::new(&bin_path)
            .arg(mode_arg)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;

        {
            let mut process_guard = state.process.lock().map_err(|e| e.to_string())?;
            *process_guard = Some(ManagedChild(Some(child)));
        }

        crate::runtime::menubar::refresh(&app);
        let _ = app.emit("awake-enabled", true);

        Ok(true)
    } else {
        stop_awake(&state);
        crate::runtime::menubar::refresh(&app);
        let _ = app.emit("awake-enabled", false);
        Ok(false)
    }
}

#[tauri::command]
pub async fn is_awake_enabled(state: State<'_, AwakeState>) -> Result<bool, String> {
    let process_guard = state.process.lock().map_err(|e| e.to_string())?;
    Ok(process_guard.is_some())
}

#[tauri::command]
pub async fn set_awake_display_mode(
    app: AppHandle,
    state: State<'_, AwakeState>,
    mode: String,
) -> Result<(), String> {
    let mirror = mode == "mirror";
    if MIRROR_MODE.load(Ordering::Relaxed) == mirror {
        return Ok(()); // 模式未变，跳过（避免前端 watch 回声触发无谓 restart）
    }
    MIRROR_MODE.store(mirror, Ordering::Relaxed);
    restart_awake(&app, &state, mirror)?;
    let _ = app.emit("awake-mode", mode);
    crate::runtime::menubar::refresh(&app);
    Ok(())
}

/// 命令注册（局部 invoke_handler）。
pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new("awake").build()
}

/// Awake 扩展。
pub struct AwakeExtension;

#[async_trait::async_trait]
impl Extension for AwakeExtension {
    fn id(&self) -> &'static str {
        "awake"
    }

    async fn setup(&self, app: &AppHandle) -> tauri::Result<()> {
        app.manage(AwakeState {
            process: Mutex::new(None),
        });
        // H12：清理旧版 awake binary（曾落 temp_dir，已迁移至 app_data_dir）。
        // 自身的向后兼容由自己负责，避免泄漏到 screenshot 等其它扩展。
        let temp_dir = std::env::temp_dir();
        let legacy_dir = temp_dir.join("com.litiantao.voidnix");
        let _ = std::fs::remove_file(legacy_dir.join("Display Wakelock"));
        let _ = std::fs::remove_dir(&legacy_dir);

        // 菜单栏贡献：保持唤醒激活时显示两项（开关 + 模式切换）
        crate::runtime::menubar::register(MenuBarContribution {
            title: "保持系统唤醒",
            build: Arc::new(build_awake),
            on_event: Arc::new(on_awake_event),
        });
        Ok(())
    }
}

/// 菜单快照：保持唤醒激活时贡献两项（启用开关 CheckItem + 显示模式二级菜单），未激活返回空。
/// 文案与界面 View.vue 保持一致（启用唤醒 / 显示模式 / 镜像 / 扩展）。
fn build_awake(app: &AppHandle) -> Vec<MenuEntry> {
    let state = app.state::<AwakeState>();
    let active = crate::runtime::lock_or_recover(&state.process).is_some();
    if !active {
        return vec![];
    }
    let mirror = MIRROR_MODE.load(Ordering::Relaxed);
    vec![
        MenuEntry::Item {
            id: "awake_open".into(),
            label: "打开扩展".into(),
            enabled: true,
        },
        MenuEntry::CheckItem {
            id: "awake_toggle".into(),
            label: "启用唤醒".into(),
            checked: true,
        },
        MenuEntry::Submenu {
            label: format!("显示模式：{}", if mirror { "镜像" } else { "扩展" }),
            items: vec![
                MenuEntry::CheckItem {
                    id: "awake_mode_mirror".into(),
                    label: "镜像".into(),
                    checked: mirror,
                },
                MenuEntry::CheckItem {
                    id: "awake_mode_extend".into(),
                    label: "扩展".into(),
                    checked: !mirror,
                },
            ],
        },
    ]
}

/// 菜单点击：打开扩展 → show_main + emit；启用开关 → 关闭；显示模式子项 → 切到对应模式。
/// 均复用命令（内部 refresh + emit 同步前端）。
fn on_awake_event(app: &AppHandle, id: &str) {
    match id {
        "awake_open" => {
            let app2 = app.clone();
            let _ = app.run_on_main_thread(move || {
                crate::runtime::window::show_main(&app2);
                let _ = app2.emit("open-module", "awake");
            });
        }
        "awake_toggle" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<AwakeState>();
                let _ = set_awake_enabled(app.clone(), state, false).await;
            });
        }
        "awake_mode_mirror" | "awake_mode_extend" => {
            let mode = if id == "awake_mode_mirror" {
                "mirror"
            } else {
                "extend"
            }
            .to_string();
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<AwakeState>();
                let _ = set_awake_display_mode(app.clone(), state, mode).await;
            });
        }
        _ => {}
    }
}
