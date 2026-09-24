use crate::platform::sleep::{self, SleepWatchdogError};
use crate::runtime::menubar::{MenuBarContribution, MenuEntry};
use crate::runtime::registry::Extension;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

/// 开关意图状态：true = flag 在场 + watchdog 持有 disablesleep。
/// 系统真实状态由 watchdog 异步对齐（2 秒轮询），此处是 UI/菜单的即时真值。
pub struct AwakeState {
    pub enabled: AtomicBool,
    /// 本 app 运行期内 watchdog 是否已获管理员授权（仅首次开启弹一次密码）
    helper_started: AtomicBool,
    /// 授权弹窗进行中守卫：弹窗期间二次开启直接拒绝，防 osascript 授权对话框叠加
    engaging: AtomicBool,
}

/// 电池护栏阈值：放电中低于此百分比即解除持有。disablesleep 会压住系统的
/// 低电量睡眠路径，合盖 + 电池 + 无人值守将一路耗到硬关机（Keepresso 实证
/// 会跑过截止线）；解除后系统随即按自身策略入睡，剩余电量在睡眠态耗速极低。
/// 一次性动作无自动恢复（重新开启由用户决定），无滞回需求。
const BATTERY_FLOOR_PERCENT: u32 = 20;

/// watchdog flag 文件路径（ext_data_dir/extensions/awake/）。按 app pid 命名：
/// 快速重启场景下旧 watchdog 的退出清理（rm flag）不会误删新实例刚写的 flag
/// （旧实例死亡到新实例 engage 落 flag 可短于旧 watchdog 的 2 秒轮询窗口），
/// 残留的旧 pid flag 由下次启动 setup 按 glob 清理。
fn flag_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(crate::runtime::storage::ext_data_dir(app, "awake")?
        .join(format!("sleep-watchdog-{}.flag", std::process::id())))
}

#[tauri::command]
pub async fn set_awake_enabled(
    app: AppHandle,
    state: State<'_, AwakeState>,
    enabled: bool,
) -> Result<bool, String> {
    if enabled {
        // 幂等：已启用（含前端 config watch 回声）直接返回
        if state.enabled.load(Ordering::Relaxed) {
            return Ok(true);
        }
        // CAS 独占：授权弹窗为模态阻塞，期间二次开启并发起第二个
        // osascript 会叠加授权对话框，直接拒绝
        if !state.engaging.swap(true, Ordering::AcqRel) {
            let result = engage(&app, &state).await;
            state.engaging.store(false, Ordering::Release);
            return result;
        }
        Err("管理员授权进行中".into())
    } else {
        // 仅删 flag：watchdog 活着时 2 秒内自动回落（本运行期内关闭过的场景）；
        // 从未启动过 watchdog 则零副作用（含前端 config 回填触发的初始 false 回声）
        let _ = flag_path(&app).and_then(|f| std::fs::remove_file(&f).map_err(|e| e.to_string()));
        state.enabled.store(false, Ordering::Relaxed);
        crate::runtime::menubar::refresh(&app);
        let _ = app.emit("awake-enabled", false);
        Ok(false)
    }
}

/// 开启路径主体（engaging 独占区内执行）。
///
/// 不变量：授权弹窗期间所有关闭入口不可达——菜单栏贡献段、电池巡检都以
/// `enabled=true` 为前提，而 engage 完成前置位前它恒为 false；View 的关闭
/// 路径同样只在开启态（toggle 值为 true）下可达。因此 engage 无需与关闭
/// 路径互斥（无「弹窗中删 flag、完成后又置 enabled」的交错）。若将来新增
/// 关闭入口（如 URL 命令），必须保持该前提或引入 disarm 计数。
async fn engage(app: &AppHandle, state: &AwakeState) -> Result<bool, String> {
    let flag = flag_path(app)?;
    // 先落 flag 再授权：watchdog 首个周期即上翻 disablesleep；
    // 授权取消/失败时回收，不留「flag 在场但无人持有」的中间态
    if let Some(parent) = flag.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&flag, b"").map_err(|e| e.to_string())?;
    if !state.helper_started.load(Ordering::Relaxed) {
        let pid = std::process::id();
        let flag_for_spawn = flag.clone();
        let started = tauri::async_runtime::spawn_blocking(move || {
            sleep::spawn_sleep_watchdog(&flag_for_spawn, pid)
        })
        .await
        .map_err(|e| e.to_string())?;
        match started {
            Ok(()) => {
                state.helper_started.store(true, Ordering::Relaxed);
            }
            Err(SleepWatchdogError::Cancelled) => {
                let _ = std::fs::remove_file(&flag);
                return Err("已取消管理员授权".into());
            }
            Err(SleepWatchdogError::Failed(msg)) => {
                let _ = std::fs::remove_file(&flag);
                return Err(format!("启动睡眠守护失败：{msg}"));
            }
        }
    }
    state.enabled.store(true, Ordering::Relaxed);
    crate::runtime::menubar::refresh(app);
    let _ = app.emit("awake-enabled", true);
    Ok(true)
}

#[tauri::command]
pub async fn is_awake_enabled(state: State<'_, AwakeState>) -> Result<bool, String> {
    Ok(state.enabled.load(Ordering::Relaxed))
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
            enabled: AtomicBool::new(false),
            helper_started: AtomicBool::new(false),
            engaging: AtomicBool::new(false),
        });
        // 历史与异常清理：旧版虚拟显示器 binary（H12 曾落 temp_dir）、
        // 断电/被杀场景残留的旧 pid flag（watchdog 随 app 退出自愈，正常路径
        // 不残留；本进程自身的 flag 尚不存在，glob 命中的必是旧实例残留）。
        // 同步执行：开销微小，且须赶在前端 config 回填触发的 engage 写 flag
        // 之前完成
        if let Ok(dir) = crate::runtime::storage::ext_data_dir(app, "awake") {
            let _ = std::fs::remove_file(dir.join("Display Wakelock"));
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    if name.to_string_lossy().starts_with("sleep-watchdog-")
                        && name.to_string_lossy().ends_with(".flag")
                    {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
        }
        let legacy_dir = std::env::temp_dir().join("com.litiantao.voidnix");
        let _ = std::fs::remove_file(legacy_dir.join("Display Wakelock"));
        let _ = std::fs::remove_dir(&legacy_dir);

        // 菜单栏贡献：保持唤醒激活时显示两项（打开扩展 + 启用开关）
        crate::runtime::menubar::register(MenuBarContribution {
            title: "保持系统唤醒",
            build: Arc::new(build_awake),
            on_event: Arc::new(on_awake_event),
        });

        // 电池护栏：60s 低频巡检，放电中低于阈值即解除（删 flag，watchdog 回落）。
        // 读取失败/插电/未启用一律 no-op；一次性解除后 enabled=false 自然停检
        let guard_app = app.clone();
        tauri::async_runtime::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                tick.tick().await;
                if !guard_app
                    .state::<AwakeState>()
                    .enabled
                    .load(Ordering::Relaxed)
                {
                    continue;
                }
                let status = tauri::async_runtime::spawn_blocking(sleep::read_battery_status)
                    .await
                    .ok()
                    .flatten();
                let Some(battery) = status else { continue };
                if battery.on_battery && battery.percent < BATTERY_FLOOR_PERCENT {
                    let _ = flag_path(&guard_app)
                        .and_then(|f| std::fs::remove_file(&f).map_err(|e| e.to_string()));
                    guard_app
                        .state::<AwakeState>()
                        .enabled
                        .store(false, Ordering::Relaxed);
                    crate::runtime::menubar::refresh(&guard_app);
                    let _ = guard_app.emit("awake-enabled", false);
                }
            }
        });
        Ok(())
    }
}

/// 菜单快照：保持唤醒激活时贡献两项（打开扩展 + 启用开关 CheckItem），未激活返回空。
/// 文案与界面 View.vue 保持一致。
fn build_awake(app: &AppHandle) -> Vec<MenuEntry> {
    let state = app.state::<AwakeState>();
    if !state.enabled.load(Ordering::Relaxed) {
        return vec![];
    }
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
    ]
}

/// 菜单点击：打开扩展 → emit（带 wasVisible）；启用开关 → 关闭。
/// 均复用命令（内部 refresh + emit 同步前端）。
fn on_awake_event(app: &AppHandle, id: &str) {
    match id {
        "awake_open" => {
            // show 由前端 listener 控制（wasVisible=false 时 setActiveExtension → rAF → showWindow），
            // 避免 Rust 立即 show 时 webview 仍为旧视图的闪现
            let was_visible = crate::runtime::shortcut::is_window_visible();
            let _ = app.emit(
                "open-extension",
                serde_json::json!({ "id": "awake", "wasVisible": was_visible }),
            );
        }
        "awake_toggle" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<AwakeState>();
                let _ = set_awake_enabled(app.clone(), state, false).await;
            });
        }
        _ => {}
    }
}
