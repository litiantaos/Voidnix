mod daemon;

use crate::platform::sleep;
use crate::runtime::menubar::{MenuBarContribution, MenuEntry};
use crate::runtime::registry::Extension;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

/// 开关意图状态：true = flag 在场 + daemon 持有 disablesleep。
/// 系统真实状态由 daemon 异步对齐（2 秒轮询），此处是 UI/菜单的即时真值。
pub struct AwakeState {
    pub enabled: AtomicBool,
    /// 授权弹窗进行中守卫：弹窗期间二次开启直接拒绝，防 osascript 授权对话框叠加
    engaging: AtomicBool,
    /// 意图互斥锁：串行化「撤意图 + 删 flag」与心跳任务的「enabled 判定 + 补写
    /// flag」——两者都 check-then-act，无锁时删除与补写交错会把刚撤下的 flag
    /// 复活（电池护栏的紧急睡眠被拖延一个 GRACE）
    intent_lock: std::sync::Mutex<()>,
    /// 进程活动持有（App Nap 豁免）：enabled 期间持有，防止合盖无交互时心跳
    /// 定时器被 timer 合并拖延过 GRACE 丢持有
    activity: std::sync::Mutex<Option<sleep::ActivityHold>>,
    /// 菜单栏快捷开关可见性（前端 config watch 同步）。菜单段不随 enabled
    /// 变化显隐——开启即常驻，CheckItem 勾选态反映 enabled。
    menubar_visible: AtomicBool,
    /// 合盖熄屏策略（前端 config watch 同步）：0=显示睡眠（displaysleepnow，
    /// 省电最优但屏幕捕获流冻结、远程停摆）；1=零亮度（背光归零、framebuffer
    /// 保持活跃，远程控制可用）
    screen_policy: AtomicU8,
}

/// 撤意图的统一收束（intent_lock 临界区内）：store false + 删 flag + 结束活动
/// 持有。所有关闭路径（命令、电池护栏、心跳降级）共用，保证三者与心跳补写
/// 互斥。refresh/emit 由调用方收尾（锁外，无重入风险）。
fn disarm(app: &AppHandle, state: &AwakeState) {
    let _guard = state.intent_lock.lock().unwrap();
    state.enabled.store(false, Ordering::Relaxed);
    daemon::remove_flag(app);
    if let Some(hold) = state.activity.lock().unwrap().take() {
        hold.end();
    }
}

/// 电池护栏阈值：放电中低于此百分比即解除持有。disablesleep 会压住系统的
/// 低电量睡眠路径，合盖 + 电池 + 无人值守将一路耗到硬关机（Keepresso 实证
/// 会跑过截止线）；解除后系统随即按自身策略入睡，剩余电量在睡眠态耗速极低。
/// 一次性动作无自动恢复（重新开启由用户决定），无滞回需求。
const BATTERY_FLOOR_PERCENT: u32 = 20;

/// dev 构建诊断日志（与 lib.rs [boot] 埋点同款门控），用于熄屏链路实测定位。
fn debug_log(message: impl FnOnce() -> String) {
    if cfg!(debug_assertions) {
        eprintln!("{}", message());
    }
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
        // 统一收束（intent_lock 临界区内撤意图 + 删 flag + 结束活动持有，与心跳
        // 补写互斥）：daemon 常驻在场，删 flag 后 2 秒内自动回落；daemon 未装时
        // 零副作用（含前端 config 回填触发的初始 false 回声）
        disarm(&app, &state);
        crate::runtime::menubar::refresh(&app);
        let _ = app.emit("awake-enabled", false);
        Ok(false)
    }
}

/// 开启路径主体（engaging 独占区内执行）。
///
/// 顺序硬不变量：ensure_daemon（可能提权弹窗，可达分钟级）→ touch beat →
/// 写 flag → 置 enabled。心跳任务仅在 enabled=true 时 touch beat，若 flag
/// 先于弹窗落盘，弹窗期间 beat 停更会被在场 daemon 判定 app 已死而自清 flag
/// （静默漂移）；写 flag 前 beat 必新鲜。取消/失败发生在写 flag 之前，零清理。
///
/// 不变量：授权弹窗期间所有关闭入口不可达——菜单栏贡献段、电池巡检都以
/// `enabled=true` 为前提，而 engage 完成前置位前它恒为 false；View 的关闭
/// 路径同样只在开启态（toggle 值为 true）下可达。因此 engage 无需与关闭
/// 路径互斥（无「弹窗中删 flag、完成后又置 enabled」的交错）。若将来新增
/// 关闭入口（如 URL 命令），必须保持该前提或引入 disarm 计数。
async fn engage(app: &AppHandle, state: &AwakeState) -> Result<bool, String> {
    daemon::ensure_daemon(app).await?;
    daemon::touch_beat(app)?;
    daemon::write_flag(app)?;
    state.enabled.store(true, Ordering::Relaxed);
    // App Nap 豁免：合盖无交互的隐藏 app 正是 timer 合并的完整画像，心跳
    // 定时器被拖延过 GRACE 即丢持有。失败按无豁免降级（心跳照常）。
    *state.activity.lock().unwrap() = sleep::begin_activity_hold();
    crate::runtime::menubar::refresh(app);
    let _ = app.emit("awake-enabled", true);
    Ok(true)
}

#[tauri::command]
pub async fn is_awake_enabled(state: State<'_, AwakeState>) -> Result<bool, String> {
    Ok(state.enabled.load(Ordering::Relaxed))
}

#[tauri::command]
pub async fn set_awake_menubar_visible(
    app: AppHandle,
    state: State<'_, AwakeState>,
    visible: bool,
) -> Result<(), String> {
    state.menubar_visible.store(visible, Ordering::Relaxed);
    crate::runtime::menubar::refresh(&app);
    Ok(())
}

#[tauri::command]
pub async fn set_awake_screen_policy(
    app: AppHandle,
    state: State<'_, AwakeState>,
    policy: String,
) -> Result<(), String> {
    let value = match policy.as_str() {
        "sleep" => 0u8,
        "dim" => 1u8,
        _ => return Err(format!("未知熄屏策略：{policy}")),
    };
    state.screen_policy.store(value, Ordering::Relaxed);
    crate::runtime::menubar::refresh(&app);
    // 菜单栏切换路径同步前端 config（watch 回声幂等）
    let _ = app.emit("awake-policy", &policy);
    Ok(())
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
            engaging: AtomicBool::new(false),
            intent_lock: std::sync::Mutex::new(()),
            activity: std::sync::Mutex::new(None),
            menubar_visible: AtomicBool::new(false),
            screen_policy: AtomicU8::new(1),
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

        // 菜单栏贡献：menubar_visible 开启时常驻启用开关（menubar_visible 由
        // 前端 config watch 经 set_awake_menubar_visible 同步，默认不显示）
        crate::runtime::menubar::register(MenuBarContribution {
            title: "保持系统唤醒",
            order: 160,
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
                    // 统一收束（与心跳补写互斥）：daemon 回落，系统随即入睡
                    disarm(&guard_app, &guard_app.state::<AwakeState>());
                    crate::runtime::menubar::refresh(&guard_app);
                    let _ = guard_app.emit("awake-enabled", false);
                }
            }
        });

        // app 心跳（镜像电池巡检范式）：enabled 时按 daemon 周期 touch beat
        //（daemon 持有的存活信号，停更 GRACE 后 daemon 自清 flag 恢复默认睡眠
        // ——app 崩溃安全的全部机制；App Nap 由 activity hold 豁免，见 engage）。
        // 两个收束分支：
        // - 自愈：enabled 而 flag 缺失（强制睡眠唤醒后 daemon 已按 beat 过期自清
        //   等漂移场景）且 daemon 活着时补写 flag 重建持有，与 daemon 侧 pmset -g
        //   校验自愈同构；daemon 已死则不补，留待下次 engage 的 ensure_daemon 重装。
        //   判定与补写整体在 intent_lock 内，与撤意图路径互斥（防把刚撤下的 flag
        //   复活）。
        // - 降级：touch 连续失败 5 次（10s，权限/路径异常）即撤意图如实上报——
        //   daemon 端会因 beat 停更自清，UI 停留在「开」即静默漂移
        let beat_app = app.clone();
        tauri::async_runtime::spawn(async move {
            let mut tick =
                tokio::time::interval(std::time::Duration::from_secs(sleep::DAEMON_CYCLE_SECS));
            let mut failures: u32 = 0;
            loop {
                tick.tick().await;
                let state = beat_app.state::<AwakeState>();
                if !state.enabled.load(Ordering::Relaxed) {
                    failures = 0;
                    continue;
                }
                if daemon::touch_beat(&beat_app).is_err() {
                    failures += 1;
                    if failures >= 5 {
                        failures = 0;
                        disarm(&beat_app, &state);
                        crate::runtime::menubar::refresh(&beat_app);
                        let _ = beat_app.emit("awake-enabled", false);
                    }
                    continue;
                }
                failures = 0;
                let _guard = state.intent_lock.lock().unwrap();
                if !daemon::flag_exists(&beat_app) && daemon::daemon_alive(&beat_app) {
                    let _ = daemon::write_flag(&beat_app);
                }
            }
        });

        // 熄屏巡检：disablesleep 挡住合盖睡眠后系统不会自动关内屏（无外接时
        // macOS 走的是睡眠路径而非 clamshell 关屏路径），须主动熄屏。合盖 +
        // 无外接的边沿立即执行、停留期节流补做；有外接屏时不动（clamshell
        // 外接显示是正常用法）。两档策略（screen_policy）：
        // - 显示睡眠：displaysleepnow 全屏级熄灭，省电最优，但屏幕捕获流
        //   随之冻结、远程控制停摆
        // - 零亮度：背光归零、framebuffer 保持活跃，远程可用；开盖期持续
        //   采样亮度作恢复目标（macOS 合盖首拍即归零，活读常为 0），开盖
        //   （或开关关闭）时面板仍暗才恢复。亮度不可控机型自动退化显示睡眠
        // 2s 轮询仅检测开销（IORegistry 合盖检测 + CG 显示列表 + DisplayServices
        // 亮度读写均为微秒级调用）
        let screen_app = app.clone();
        tauri::async_runtime::spawn(async move {
            use sleep::ScreenDimAction;
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(2));
            let mut was_closed = false;
            let mut last_sleep_at: Option<std::time::Instant> = None;
            // 零亮度策略状态：开盖期亮度样本 / 恢复目标 / 上次压零时刻
            let mut open_sample: Option<f32> = None;
            let mut saved_target: Option<f32> = None;
            let mut last_dim_at: Option<std::time::Instant> = None;
            let since = |at: Option<std::time::Instant>| {
                at.map(|t| std::time::Instant::now() - t)
                    .unwrap_or(std::time::Duration::MAX)
            };
            loop {
                tick.tick().await;
                let enabled = screen_app
                    .state::<AwakeState>()
                    .enabled
                    .load(Ordering::Relaxed);
                let policy = screen_app
                    .state::<AwakeState>()
                    .screen_policy
                    .load(Ordering::Relaxed);
                let lid = sleep::lid_is_closed();
                let external = sleep::has_external_display();

                // 开关关闭后的兜底恢复：零亮度残留时无条件还亮度再退场
                // （否则合盖态关开关会让开盖黑屏）
                if !enabled {
                    if let Some(target) = saved_target.take() {
                        if sleep::builtin_brightness()
                            .map(|b| b <= sleep::BRIGHTNESS_LIT_THRESHOLD)
                            .unwrap_or(false)
                        {
                            let ok = sleep::set_builtin_brightness(target);
                            debug_log(|| {
                                format!("[awake] dim restore on-disable target={target} ok={ok}")
                            });
                        }
                    }
                    was_closed = false;
                    last_sleep_at = None;
                    last_dim_at = None;
                    continue;
                }

                let brightness = sleep::builtin_brightness();
                // 亮度恢复统一出口（独立于当前策略）：零亮度持有期间开盖即还
                // 亮度——策略中途切换（dim→sleep）不能把恢复责任丢给已离开的
                // 分支。外部已调亮的跳过并丢弃
                if saved_target.is_some() && lid == Some(false) {
                    if let Some(target) = saved_target.take() {
                        if brightness
                            .map(|b| b <= sleep::BRIGHTNESS_LIT_THRESHOLD)
                            .unwrap_or(false)
                        {
                            let ok = sleep::set_builtin_brightness(target);
                            debug_log(|| {
                                format!("[awake] dim restore on-open target={target} ok={ok}")
                            });
                        }
                    }
                    last_dim_at = None;
                }

                let dim = policy == 1 && brightness.is_some();
                if dim {
                    // 开盖期持续采样亮读数（暗读跳过）作恢复目标
                    if lid == Some(false) {
                        if let Some(b) = brightness.filter(|b| *b > sleep::BRIGHTNESS_LIT_THRESHOLD)
                        {
                            open_sample = Some(b);
                        }
                    }
                    match sleep::screen_dim_action(
                        lid,
                        was_closed,
                        external,
                        brightness,
                        open_sample,
                        saved_target.is_some(),
                        since(last_dim_at),
                    ) {
                        ScreenDimAction::Zero { target } => {
                            saved_target = Some(target);
                            let ok = sleep::set_builtin_brightness(0.0);
                            last_dim_at = Some(std::time::Instant::now());
                            debug_log(|| {
                                format!(
                                    "[awake] dim zero target={target} ok={ok} lid={lid:?} external={external}"
                                )
                            });
                        }
                        ScreenDimAction::Rezero => {
                            let ok = sleep::set_builtin_brightness(0.0);
                            last_dim_at = Some(std::time::Instant::now());
                            debug_log(|| format!("[awake] dim rezero ok={ok} (externally lit)"));
                        }
                        // Restore 已由上方统一出口处理，这里不会再命中
                        ScreenDimAction::Restore | ScreenDimAction::None => {}
                    }
                } else if sleep::screen_sleep_due(lid, external, was_closed, since(last_sleep_at)) {
                    sleep::sleep_displays_now();
                    last_sleep_at = Some(std::time::Instant::now());
                    debug_log(|| {
                        format!(
                            "[awake] display sleep lid={lid:?} external={external} policy={policy}"
                        )
                    });
                }
                // lid 读 None（AppleClamshellState 偶发 flutter）保持闭锁，
                // 防止 flutter 制造伪开盖→伪边沿重复熄屏
                if let Some(closed) = lid {
                    was_closed = closed;
                }
            }
        });
        Ok(())
    }
}

/// 菜单快照：`menubar_visible` 开启时常驻贡献启用开关 + 熄屏方式二级菜单
/// （勾选态分别反映 enabled 与当前策略，不随状态显隐）；关闭时返回空段。
/// 文案与界面 View.vue 保持一致。
fn build_awake(app: &AppHandle) -> Vec<MenuEntry> {
    let state = app.state::<AwakeState>();
    if !state.menubar_visible.load(Ordering::Relaxed) {
        return vec![];
    }
    let dim = state.screen_policy.load(Ordering::Relaxed) == 1;
    vec![
        MenuEntry::CheckItem {
            id: "awake_toggle".into(),
            label: "启用唤醒".into(),
            checked: state.enabled.load(Ordering::Relaxed),
        },
        MenuEntry::Submenu {
            label: "熄屏方式".into(),
            items: vec![
                MenuEntry::CheckItem {
                    id: "awake_policy_dim".into(),
                    label: "零亮度（远程可用）".into(),
                    checked: dim,
                },
                MenuEntry::CheckItem {
                    id: "awake_policy_sleep".into(),
                    label: "显示睡眠（更省电）".into(),
                    checked: !dim,
                },
            ],
        },
    ]
}

/// 菜单点击：启用开关按当前状态取反（开启路径经授权，取消/进行中静默）；
/// 熄屏策略项切换到对应档位。均复用命令（内部 refresh + emit 同步前端）。
fn on_awake_event(app: &AppHandle, id: &str) {
    if id == "awake_toggle" {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let state = app.state::<AwakeState>();
            let next = !state.enabled.load(Ordering::Relaxed);
            let _ = set_awake_enabled(app.clone(), state, next).await;
        });
    } else if id == "awake_policy_dim" || id == "awake_policy_sleep" {
        let policy = if id == "awake_policy_dim" {
            "dim"
        } else {
            "sleep"
        };
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let state = app.state::<AwakeState>();
            let _ = set_awake_screen_policy(app.clone(), state, policy.to_string()).await;
        });
    }
}
