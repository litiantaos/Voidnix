mod daemon;
mod privacy_hold;

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
    /// 开盖零亮度/隐私屏（会话级，不落盘）：开盖时把内置屏背光归零并持有显示睡眠豁免，
    /// 供办公室/公共场所手机远控时遮挡屏幕内容。合盖时由 screenPolicy 独立决定，
    /// 按盖子上下文各自生效；app 重启后归 false（残留亮度由 setup 兜底恢复）。
    privacy: AtomicBool,
}

/// 撤意图的统一收束（intent_lock 临界区内）：store false + 删 flag + 结束活动
/// 持有。所有关闭路径（命令、电池护栏、心跳降级）共用，保证三者与心跳补写
/// 互斥。refresh/emit 由调用方收尾（锁外，无重入风险）。
fn disarm(app: &AppHandle, state: &AwakeState) {
    let _guard = state.intent_lock.lock().unwrap();
    state.enabled.store(false, Ordering::Relaxed);
    // 隐私屏依赖系统保持唤醒；解除持有（命令关闭 / 电池护栏 / 心跳降级）时
    // 一并撤销，由巡检恢复亮度。View 监听 awake-enabled=false 同步 UI。
    state.privacy.store(false, Ordering::Relaxed);
    privacy_hold::stop(app);
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

/// 开盖时屏幕零亮度开关主体（命令 / 菜单 / 设置页共用）。
///
/// 开启时先校验依赖与显示能力：保持系统唤醒必须已开启（本函数**不自动
/// 开启** wake，避免「开屏幕顺带改系统状态」），未开启时明确拒绝并提示；
/// 外接屏无法用内置屏亮度 API 遮挡；无内置屏（合盖时常见）同样拒绝。
/// 关闭只撤开盖零亮度，不自动关闭 wake（用户可能仍需远控）；合盖屏幕由
/// screenPolicy 独立决定。
async fn apply_privacy(app: &AppHandle, enabled: bool) -> Result<bool, String> {
    if enabled {
        if !app.state::<AwakeState>().enabled.load(Ordering::Relaxed) {
            return Err("请先开启「系统唤醒」；开盖零亮度依赖它保持远控".into());
        }
        if sleep::has_external_display() {
            return Err("检测到外接显示器，零亮度模式无法遮挡外接屏".into());
        }
        if sleep::builtin_brightness().is_none() {
            return Err("当前没有可控制的内置显示器".into());
        }
    }
    let state = app.state::<AwakeState>();
    state.privacy.store(enabled, Ordering::Relaxed);
    if enabled {
        privacy_hold::start(app);
    } else {
        privacy_hold::stop(app);
    }
    crate::runtime::menubar::refresh(app);
    let _ = app.emit("awake-privacy", enabled);
    Ok(enabled)
}

#[tauri::command]
pub async fn set_awake_privacy(app: AppHandle, enabled: bool) -> Result<bool, String> {
    apply_privacy(&app, enabled).await
}

#[tauri::command]
pub async fn is_awake_privacy(state: State<'_, AwakeState>) -> Result<bool, String> {
    Ok(state.privacy.load(Ordering::Relaxed))
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
            privacy: AtomicBool::new(false),
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

        // 崩溃/强杀后零亮度残留兜底：新进程没有 saved_target，若亮度仍为 0
        // （上轮隐私屏或合盖 dim 未恢复），恢复到安全亮度——隐私屏是会话级
        // 状态，重启即退出，不能把用户留在黑屏。
        if let Some(brightness) = sleep::builtin_brightness() {
            if brightness <= sleep::BRIGHTNESS_LIT_THRESHOLD {
                let ok = sleep::set_builtin_brightness(sleep::DEFAULT_RESTORE_BRIGHTNESS);
                debug_log(|| format!("[awake] startup brightness restore ok={ok}"));
            }
        }

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
            // 零亮度持有期间的显示空闲睡眠豁免：disablesleep 只挡系统睡眠，
            // displaysleep 定时器（及 UU 自身的 30min UserIsActive 超时）仍会
            // 关显示子系统、冻结捕获流。见 platform::sleep::DisplaySleepHold。
            let mut display_hold: Option<sleep::DisplaySleepHold> = None;
            let since = |at: Option<std::time::Instant>| {
                at.map(|t| std::time::Instant::now() - t)
                    .unwrap_or(std::time::Duration::MAX)
            };
            loop {
                tick.tick().await;
                let (enabled, mut privacy, policy) = {
                    let state = screen_app.state::<AwakeState>();
                    (
                        state.enabled.load(Ordering::Relaxed),
                        state.privacy.load(Ordering::Relaxed),
                        state.screen_policy.load(Ordering::Relaxed),
                    )
                };
                let lid = sleep::lid_is_closed();
                let external = sleep::has_external_display();

                // 开关关闭后的兜底恢复：零亮度残留时无条件还亮度再退场
                // （否则合盖态关开关会让开盖黑屏）
                if !enabled {
                    display_hold = None;
                    // disarm 已清隐私；这里兜底并发竞态（电池护栏/心跳降级与
                    // apply_privacy 交错时可能在校验后又被置 true），避免
                    // 「UI 显示隐私开、系统已撤持有」的静默漂移。
                    if screen_app
                        .state::<AwakeState>()
                        .privacy
                        .swap(false, Ordering::Relaxed)
                    {
                        privacy_hold::stop(&screen_app);
                        let _ = screen_app.emit("awake-privacy", false);
                        crate::runtime::menubar::refresh(&screen_app);
                    }
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

                // 外接到场 fail-open：内置屏亮度 API 遮不住外接屏，继续开着
                // 会让用户误以为内容不可见——立即关闭并恢复屏幕。
                if privacy && external {
                    screen_app
                        .state::<AwakeState>()
                        .privacy
                        .store(false, Ordering::Relaxed);
                    privacy = false;
                    privacy_hold::stop(&screen_app);
                    let _ = screen_app.emit("awake-privacy", false);
                    let _ = screen_app.emit("awake-menu-failed", "检测到外接显示器，零亮度已关闭");
                    crate::runtime::menubar::refresh(&screen_app);
                }

                let lid_closed = lid.unwrap_or(was_closed);
                // 开盖零亮度只作用于开盖；合盖屏幕由 screenPolicy 独立决定。
                let privacy_open = privacy && !lid_closed;

                let brightness = sleep::builtin_brightness();
                // 亮度恢复统一出口（独立于当前策略）：零亮度持有期间开盖即还
                // 亮度——策略中途切换（dim→sleep）不能把恢复责任丢给已离开的
                // 分支。外部已调亮的跳过并丢弃
                if !privacy && saved_target.is_some() && lid == Some(false) {
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

                // 显示空闲睡眠豁免：系统 displaysleep 定时器与远控自身的 30min
                // UserIsActive 超时都会关显示子系统、冻结捕获流。开盖零亮度必须
                // 持有；合盖 dim 仍只在「合盖 + 无外接」持有（sleep 策略不持有）。
                // 开盖 / 切 sleep 策略 / 外接到场即释放。
                let hold_display = brightness.is_some()
                    && (privacy_open
                        || sleep::should_hold_display_awake(
                            policy, lid, was_closed, external, brightness,
                        ));
                if hold_display {
                    if display_hold.is_none() {
                        if let Some(hold) = sleep::DisplaySleepHold::acquire() {
                            debug_log(|| "[awake] display sleep hold acquired".to_string());
                            display_hold = Some(hold);
                        }
                    }
                } else if display_hold.take().is_some() {
                    debug_log(|| "[awake] display sleep hold released".to_string());
                }

                if privacy_open {
                    // 开盖零亮度：只作用于开盖；合盖屏幕走下方 screenPolicy。
                    // 恢复目标取当前亮读数 → 最近开盖样本 → 安全默认值；不能
                    // 像 dim 边沿那样「无目标就不动」，否则关闭后会留在黑屏。
                    saved_target.get_or_insert_with(|| {
                        sleep::privacy_restore_target(brightness, open_sample)
                    });
                    if brightness
                        .map(|b| b > sleep::BRIGHTNESS_LIT_THRESHOLD)
                        .unwrap_or(false)
                    {
                        let ok = sleep::set_builtin_brightness(0.0);
                        last_dim_at = Some(std::time::Instant::now());
                        debug_log(|| {
                            format!("[awake] privacy zero ok={ok} lid={lid:?} external={external}")
                        });
                    }
                } else {
                    let dim = policy == 1 && brightness.is_some();
                    if dim {
                        // 开盖期持续采样亮读数（暗读跳过）作恢复目标
                        if lid == Some(false) {
                            if let Some(b) =
                                brightness.filter(|b| *b > sleep::BRIGHTNESS_LIT_THRESHOLD)
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
                                debug_log(|| {
                                    format!("[awake] dim rezero ok={ok} (externally lit)")
                                });
                            }
                            // Restore 已由上方统一出口处理，这里不会再命中
                            ScreenDimAction::Restore | ScreenDimAction::None => {}
                        }
                    } else if sleep::screen_sleep_due(
                        lid,
                        external,
                        was_closed,
                        since(last_sleep_at),
                    ) {
                        // pmset 同步等退出回收子进程，阻塞调用走 spawn_blocking（电池巡检同范式）
                        let _ =
                            tauri::async_runtime::spawn_blocking(sleep::sleep_displays_now).await;
                        last_sleep_at = Some(std::time::Instant::now());
                        debug_log(|| {
                            format!(
                                "[awake] display sleep lid={lid:?} external={external} policy={policy}"
                            )
                        });
                    }
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

/// 菜单快照：`menubar_visible` 开启时按扩展分组常驻贡献「保持系统唤醒」段。
/// 未开启保持系统唤醒时只给总开关；开启后再追加开盖时屏幕 / 合盖时屏幕
/// 二级菜单（勾选态分别反映 enabled / privacy / 当前策略）。关闭时返回空段。
/// 文案与界面 View.vue 保持一致。
fn build_awake(app: &AppHandle) -> Vec<MenuEntry> {
    let state = app.state::<AwakeState>();
    if !state.menubar_visible.load(Ordering::Relaxed) {
        return vec![];
    }
    let enabled = state.enabled.load(Ordering::Relaxed);
    let dim = state.screen_policy.load(Ordering::Relaxed) == 1;
    let mut items = vec![MenuEntry::CheckItem {
        id: "awake_toggle".into(),
        label: "开启系统唤醒".into(),
        checked: enabled,
    }];
    // 屏幕项依赖保持系统唤醒；未开启时只显示总开关，避免“设置了但无效”。
    if !enabled {
        return items;
    }
    items.extend([
        MenuEntry::Submenu {
            label: "开盖时屏幕".into(),
            items: vec![
                MenuEntry::CheckItem {
                    id: "awake_open_auto".into(),
                    label: "跟随系统".into(),
                    checked: !state.privacy.load(Ordering::Relaxed),
                },
                MenuEntry::CheckItem {
                    id: "awake_open_zero".into(),
                    label: "零亮度（隐私/远控）".into(),
                    checked: state.privacy.load(Ordering::Relaxed),
                },
            ],
        },
        MenuEntry::Submenu {
            label: "合盖时屏幕".into(),
            items: vec![
                MenuEntry::CheckItem {
                    id: "awake_policy_sleep".into(),
                    label: "显示睡眠（更省电）".into(),
                    checked: !dim,
                },
                MenuEntry::CheckItem {
                    id: "awake_policy_dim".into(),
                    label: "零亮度（远程可用）".into(),
                    checked: dim,
                },
            ],
        },
    ]);
    items
}

/// 菜单点击：保持系统唤醒按当前状态取反；开盖屏幕 auto/zero 切换（zero
/// 需保持系统唤醒已开启）；合盖屏幕策略项切换到对应档位。均复用命令（内部
/// refresh + emit 同步前端）。失败 emit 给
/// 前端 toast（外部 NSPanel，菜单操作时主窗隐藏态照常可见——此前完全静默，
/// 勾选态不动无从排查）；用户主动取消授权静默（elevate 取消分类即文案化，
/// 含「取消」过滤）。
fn on_awake_event(app: &AppHandle, id: &str) {
    if id == "awake_toggle" {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let state = app.state::<AwakeState>();
            let next = !state.enabled.load(Ordering::Relaxed);
            if let Err(e) = set_awake_enabled(app.clone(), state, next).await {
                emit_menu_failure(&app, &e);
            }
        });
    } else if id == "awake_open_auto" || id == "awake_open_zero" {
        let app = app.clone();
        let next = id == "awake_open_zero";
        tauri::async_runtime::spawn(async move {
            if let Err(e) = apply_privacy(&app, next).await {
                emit_menu_failure(&app, &e);
            }
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
            if let Err(e) = set_awake_screen_policy(app.clone(), state, policy.to_string()).await {
                emit_menu_failure(&app, &e);
            }
        });
    }
}

fn emit_menu_failure(app: &AppHandle, e: &str) {
    if e.contains("取消") {
        return;
    }
    let _ = app.emit("awake-menu-failed", e);
}
