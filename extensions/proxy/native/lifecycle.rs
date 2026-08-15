//! 代理核心生命周期：ProxyState + root mihomo 启停/热重载/健康监测/启动复用。
//! 命令入口见 mod.rs；菜单栏见 menu.rs。

use super::controller;
use super::core::{self, RunParams};
use super::stream::StreamRegistry;
use super::subscription;
use super::tun;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// 代理运行状态：enabled（流量是否被代理）+ tun_active（root mihomo 进程是否在跑，常驻）
/// + run_params（最近一次 active 参数，供热重载/复用）+ 健康监测代际（见 monitor_gen 字段）。
pub struct ProxyState {
    pub enabled: AtomicBool,
    pub run_params: Mutex<Option<RunParams>>,
    pub tun_active: AtomicBool,
    /// 菜单栏状态行当前节点名缓存；由 refresh_proxy_menu 异步拉取填充。
    pub current_node: Mutex<Option<String>>,
    /// 健康监测代际：stop/reset 自增使在跑 task 代际失配而退出。替代共享 bool 标志——
    /// bool 下旧 task 醒来读到新 task 置位的 true 会「复活」（stop 后 30s 内重开 → 双
    /// monitor 并存，重复探针/重复恢复 reload/重复 toast，且旧 task 退出还会清掉新标志）。
    pub monitor_gen: AtomicU64,
    /// 已 spawn task 的代际（ensure_monitor 幂等跳过依据）。
    pub monitor_spawned_gen: AtomicU64,
    /// TUN 释放重试代际：stop_core 的后台重试捕获注册时的代际，start_core 入口自增使
    /// 旧重试作废——在源头消灭「重试 PUT idle 落在重开的 PUT active 之后」的竞态窗口
    /// （enabled 标志在 start_core 末尾才置位，靠它拦截存在缝隙）。
    pub release_gen: AtomicU64,
}

/// 健康事件 payload（emit "proxy-status"，前端 showStatus 反馈）。
#[derive(Clone, serde::Serialize)]
pub(crate) struct ProxyStatus {
    /// "success" | "error"：对齐 StatusBar kind 语义，错误必须 error 避免绿色对勾错位。
    pub kind: String,
    pub msg: String,
}

/// netstat 路由表中是否已有 TUN auto-route 路由。
/// 两代风格均须识别：老版半路由 `0/1` + `128/1`；新版路由树分解 `1` + `2/7` + `4/6` +
/// `8/5` + `16/4` + `32/3` + `64/2` + `128.0/1`（覆盖 1.0.0.0–255.255.255.255，避开
/// 0.0.0.0/8）。两者均为代理工具 auto-route 专属目标，常规网络不会出现（`127` 回环、
/// `169.254` 链路本地等不在标记集）。只匹配 IPv4 行（IPv6 分解树与运营商原生 v6 路由
/// 难区分，保守不匹配）。
fn has_tun_routes(routes: &str) -> bool {
    const MARKERS: [&str; 10] = [
        "0/1", "128/1", "1", "2/7", "4/6", "8/5", "16/4", "32/3", "64/2", "128.0/1",
    ];
    routes.lines().any(|line| {
        let dest = line.split_whitespace().next().unwrap_or("");
        MARKERS.contains(&dest)
    })
}

/// 查系统 TUN auto-route 路由。`Some(true)`=存在，`Some(false)`=不存在，`None`=netstat 失败
/// （预检路径降级放行——让 mihomo 尝试 + verify 兜底；让渡轮询按「未撤除」继续等）。
fn tun_routes() -> Option<bool> {
    let out = std::process::Command::new("netstat")
        .args(["-rn"])
        .output()
        .ok()?;
    Some(has_tun_routes(&String::from_utf8_lossy(&out.stdout)))
}

/// 检测系统是否已有 TUN auto-route 路由（老版半路由 / 新版路由树分解，见 `has_tun_routes`）。
/// mihomo 创建 TUN 必然与既有 auto-route 冲突（add route: file exists）——提前拦截给出明确提示。
/// 仅 start_core（idle→active 切换）调用，mihomo 自身 TUN 路由不会误判（idle 无 TUN 路由）。
fn tun_route_conflict() -> Option<String> {
    tun_routes()
        .filter(|&conflict| conflict)
        .map(|_| "系统已有其他代理工具的 TUN 路由，请先关闭它".into())
}

// ── 对端变体（dev/prod）TUN 让渡 ──

/// 由 bundle identifier 推导对端变体 identifier：dev 恒以 `.dev` 结尾
/// （tauri.dev.conf.json），prod 恒不带；dev→prod 去后缀，prod→dev 加后缀。
fn sibling_identifier(ident: &str, is_dev: bool) -> Option<String> {
    if is_dev {
        ident.strip_suffix(".dev").map(String::from)
    } else {
        Some(format!("{ident}.dev"))
    }
}

/// 对端变体 proxy 数据目录（`~/Library/Application Support/<sibling-id>/extensions/proxy`）。
fn sibling_proxy_dir(app: &AppHandle) -> Option<PathBuf> {
    let data = app.path().app_data_dir().ok()?;
    let sibling = sibling_identifier(&app.config().identifier, cfg!(debug_assertions))?;
    Some(
        data.parent()?
            .join(sibling)
            .join("extensions")
            .join("proxy"),
    )
}

/// 读对端 config.json 的 controller 凭证（端口 + secret），供让渡热重载。
/// 端口按对端视角归一化（对端 config.json 可能残留本端默认端口，同本端污染场景）。
fn sibling_controller_creds(dir: &Path) -> Option<(u16, String)> {
    let text = std::fs::read_to_string(dir.join("config.json")).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    let mut mixed_port = v.get("mixedPort")?.as_u64()? as u16;
    let mut controller_port = v.get("controllerPort")?.as_u64()? as u16;
    core::correct_ports_toward(
        &mut mixed_port,
        &mut controller_port,
        !cfg!(debug_assertions),
    );
    Some((controller_port, v.get("secret")?.as_str()?.to_string()))
}

/// TUN 让渡分布式通知名（NSDistributedNotificationCenter，发布者 object = 变体 bundle id）。
/// 对端 app 在跑时毫秒级收到推送、即时对账复位——轮询只能做到秒级且需常驻循环。
pub(crate) const TUN_TAKEN_NOTIFY: &str = "com.litiantao.voidnix.proxy.tun-taken";

/// 对端变体 TUN 让渡：占用者是 Voidnix 对端变体的 mihomo 时，经其 controller API
/// 热重载其磁盘上的恒 idle `config.yaml`（免提权），TUN 随之拆除，本端随后接管。
/// 让渡成功即发布分布式通知（见 `TUN_TAKEN_NOTIFY`），对端 app 若在跑则即时对账复位。
///
/// TUN 系统独占，但 dev/prod 互为「自家可控实例」（数据目录/端口/secret 全部可推导读取），
/// 对端残留 active（app 退出后 launchd 继续托管）占住 TUN 时不应按第三方工具报错让用户
/// 手动处理——app 退出后对端 UI 已不在，用户恰恰无从关闭。对端不可控（凭证缺失/secret
/// 不匹配）或占用者是第三方工具时返回 `fallback_msg`（原冲突提示）。
///
/// 占有判定用 `GET /configs` 的 `tun.enable` 而非仅看进程在跑：对端 idle 常驻是常态
/// （不占 TUN），进程在跑不等于占用者；字段缺失/读取失败不视为占用者（走第三方报错路径）。
async fn release_sibling_tun(app: &AppHandle, fallback_msg: String) -> Result<(), String> {
    let Some(dir) = sibling_proxy_dir(app) else {
        return Err(fallback_msg);
    };
    if !mihomo_running(&dir) {
        return Err(fallback_msg); // 对端未跑 → 占用者是第三方工具
    }
    let variant = if cfg!(debug_assertions) {
        "正式版"
    } else {
        "dev 版"
    };
    let Some((port, secret)) = sibling_controller_creds(&dir) else {
        return Err(format!(
            "{variant} Voidnix 的代理仍占用 TUN，请打开 {variant} 关闭代理后重试"
        ));
    };
    let base = format!("http://127.0.0.1:{port}");
    match controller::check_auth(&base, &secret).await {
        Ok(true) => {}
        _ => {
            return Err(format!(
                "{variant} Voidnix 的代理核心不可控，请打开 {variant} 关闭代理后重试"
            ))
        }
    }
    let holds_tun = controller::get_configs(&base, &secret)
        .await
        .ok()
        .and_then(|v| {
            v.get("tun")
                .and_then(|t| t.get("enable"))
                .and_then(Value::as_bool)
        })
        == Some(true);
    if !holds_tun {
        return Err(fallback_msg); // 对端 idle 常驻 → 占用者是第三方工具
    }
    // 让渡：PUT 对端磁盘上的恒 idle config.yaml（本端只读访问对端目录，不写不删）
    let idle = dir.join("config.yaml").display().to_string();
    controller::reload_config(&base, &secret, &idle, Duration::from_secs(5), 1).await?;
    // 轮询等 auto-route 路由撤除（PUT 同步应用 TUN 拆除，netstat 刷新有滞后）
    for _ in 0..15 {
        if tun_routes() != Some(true) {
            // 推送对端即时对账（object = 本端 bundle id，对端按 object 过滤观察、自收排除）
            crate::platform::distributed::post(TUN_TAKEN_NOTIFY, &app.config().identifier);
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    Err(format!("{variant} Voidnix 释放 TUN 超时，请稍后重试"))
}

/// setup 内注册对端让渡通知观察（进程生命周期）：收到推送 → 异步对账（验证真相后复位，
/// 通知可能伪造/陈旧，一律以运行 config 为准）。
pub(crate) fn observe_tun_taken(app: &AppHandle) {
    let Some(sibling) = sibling_identifier(&app.config().identifier, cfg!(debug_assertions)) else {
        return;
    };
    let app2 = app.clone();
    crate::platform::distributed::observe_on_main(
        app,
        TUN_TAKEN_NOTIFY,
        &sibling,
        std::sync::Arc::new(move || {
            let app3 = app2.clone();
            tauri::async_runtime::spawn(async move {
                reconcile_after_takeover(&app3).await;
            });
        }),
    );
}

/// 让渡通知对账：enabled 且运行 config 的 tun.enable 已关闭 → 复位 + 精确提示。
pub(crate) async fn reconcile_after_takeover(app: &AppHandle) {
    let state = app.state::<ProxyState>();
    if !state.enabled.load(Ordering::Relaxed) {
        return;
    }
    let Some((port, secret)) = controller_creds_opt(&state) else {
        return;
    };
    let base = format!("http://127.0.0.1:{port}");
    if tun_disabled(&base, &secret).await {
        reset_dead_state(app, "TUN 已被另一版本 Voidnix 接管，代理已断开，请重新开启");
    }
}

/// 回滚 idle config：先写磁盘（不经 API，确保 config.yaml 更新为 idle），再短超时热重载。
/// verify_tun_active 失败时调用——即使热重载失败（controller 卡住），磁盘已是 idle config，
/// mihomo 下次重启（launchd KeepAlive / 崩溃自愈）时自动加载 idle → 不再崩溃循环 → controller 恢复。
pub(crate) async fn rollback_to_idle(app: &AppHandle, params: &RunParams) {
    let idle = RunParams {
        mode: "direct".into(),
        tun: false,
        ..params.clone()
    };
    if let Ok(path) = write_run_config(app, &idle) {
        let base = format!("http://127.0.0.1:{}", params.controller_port);
        let _ = controller::reload_config(
            &base,
            &params.secret,
            &path.to_string_lossy(),
            Duration::from_secs(3),
            1,
        )
        .await;
    }
}

/// 写 config 到磁盘，返回路径。active→config-active.yaml（热重载专用），
/// idle→config.yaml（mihomo 启动配置，永不含 TUN 段——崩溃后 launchd 重启只加载 idle）。
fn write_run_config(app: &AppHandle, params: &RunParams) -> Result<PathBuf, String> {
    let yaml = subscription::build_run_config(app, params)?;
    let path = if params.tun {
        core::active_config_path(app)?
    } else {
        core::run_config_path(app)?
    };
    std::fs::write(&path, yaml).map_err(|e| e.to_string())?;
    Ok(path)
}

/// 写 config 到磁盘 + PUT /configs 热重载（active/idle 切换、订阅变更共用）。
/// active→config-active.yaml，idle→config.yaml（路径由 write_run_config 按 tun 字段路由）。
/// TUN 模式下 root mihomo 常驻，代理开关 = 热重载 active/idle config，免 spawn 免提权。
pub(crate) async fn reload_config_yaml(app: &AppHandle, params: &RunParams) -> Result<(), String> {
    let path = write_run_config(app, params)?;
    let base = format!("http://127.0.0.1:{}", params.controller_port);
    controller::reload_config(
        &base,
        &params.secret,
        &path.to_string_lossy(),
        Duration::from_secs(10),
        3,
    )
    .await
}

/// 读 mihomo.log 当前字节大小。reload 前快照，供 verify_tun_active 区分新增行。
pub(crate) fn log_size(app: &AppHandle) -> u64 {
    crate::runtime::storage::ext_data_dir(app, "proxy")
        .ok()
        .and_then(|d| std::fs::metadata(d.join("mihomo.log")).ok())
        .map(|m| m.len())
        .unwrap_or(0)
}

/// mihomo.log 尾读窗口（字节）：TUN 诊断只需 reload 后新增的十几行；日志无轮转、可无限
/// 增长（info 级别每连接一行），全量 read_to_string 会随体积线性放大每次开关代理的内存尖峰。
const LOG_TAIL_WINDOW: u64 = 64 * 1024;

/// mihomo.log 体积上限：超限时 stop_core（低频、用户主动关代理）截断为空——launchd 以
/// O_APPEND 持有 fd，截断后写入继续追加到新 EOF，无需重启进程。
const LOG_MAX_BYTES: u64 = 5 * 1024 * 1024;

/// 读 `since` 字节偏移之后的新增完整行；自 `since` 起超出窗口时退化为最后窗口内的完整行
/// （连接风暴下新增超 64KB 时只看最新一段，TUN error 必在尾部）。
/// 语义对齐旧 `tail_after`：丢弃 since 起的首个行段（快照时该行可能尚未写完）。
pub(crate) fn read_log_tail(path: &Path, since: u64) -> Vec<String> {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let Ok(len) = f.metadata().map(|m| m.len()) else {
        return Vec::new();
    };
    if len <= since {
        return Vec::new(); // 无新增（或已被截断/轮转）
    }
    let start = since.max(len.saturating_sub(LOG_TAIL_WINDOW));
    if f.seek(SeekFrom::Start(start)).is_err() {
        return Vec::new();
    }
    let mut bytes = Vec::new();
    if f.read_to_end(&mut bytes).is_err() {
        return Vec::new();
    }
    // 窗口起点可能落在多字节 UTF-8 序列中间，lossy 换替换符（残行随首行段一并丢弃）
    let buf = String::from_utf8_lossy(&bytes);
    let body = match buf.find('\n') {
        Some(i) => &buf[i + 1..],
        None => return Vec::new(), // since 之后无完整行
    };
    body.lines().map(str::to_string).collect()
}

/// mihomo.log 超限截断（stop_core 低频点调用，见 `LOG_MAX_BYTES`）。
fn truncate_log_if_large(dir: &Path) {
    let log = dir.join("mihomo.log");
    if std::fs::metadata(&log)
        .map(|m| m.len() > LOG_MAX_BYTES)
        .unwrap_or(false)
    {
        let _ = std::fs::File::create(&log);
    }
}

/// 热重载 active config（含 TUN）后验证 TUN 是否成功创建。
///
/// mihomo PUT /configs 返回成功不代表 TUN 创建成功——别的代理工具占着 TUN 设备/路由时，
/// mihomo 创建 TUN 静默失败（API 仍 204），流量实际未被接管。这是最危险的静默失效：
/// 用户以为代理开了但裸奔。idle config 无 TUN 段不会产生 TUN error 日志，故热重载后读
/// mihomo.log 尾部检测 TUN error 必为本次 active 创建失败。
///
/// `since` 为 reload 前日志字节偏移，只检测此后新增的行——避免进程未重启时陈旧 TUN error
/// 日志（上次失败尝试）残留在文件尾部导致误报。
///
/// **200ms buffer**：PUT /configs 同步完成 config 热重载（含 TUN 创建），返回时日志已写入
/// （实测 PUT active 37ms，返回即 `tun.enable=True` + 日志就绪）。200ms 是文件系统刷新保守
/// buffer，非等待 mihomo 处理。
pub(crate) async fn verify_tun_active(app: &AppHandle, since: u64) -> Result<(), String> {
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let Ok(dir) = crate::runtime::storage::ext_data_dir(app, "proxy") else {
        return Ok(()); // 路径异常不阻塞（reload 已成功为前提）
    };
    let lines = read_log_tail(&dir.join("mihomo.log"), since);
    for line in lines.iter().rev().take(15) {
        let l = line.to_lowercase();
        // 正常日志 "[TUN] Tun adapter listening at: utun4" 无 error 关键词不会误报。
        // route 错误（"route: file exists"）也需捕获——auto-route 路由冲突是 TUN 失败的主因。
        if (l.contains("tun") || l.contains("route"))
            && (l.contains("error") || l.contains("exist") || l.contains("fail"))
        {
            return Err("TUN 网卡或路由被其他代理工具占用，请先关闭它".into());
        }
    }
    Ok(())
}

/// 确保 root mihomo 在跑（launchd 托管）。优先级：
/// 1. controller 可达 + secret 匹配 → 复用常驻进程（幂等，免提权）
/// 2. plist 已装但 controller 不可达（开机后/崩溃重启中）→ 等 launchd 拉起（KeepAlive）
/// 3. plist 未装 → 首次 install_launchdaemon（osascript 提权一次），mihomo 启动跑 idle config
///
/// launchd 托管后 mihomo 永驻（RunAtLoad 开机自启 + KeepAlive 崩溃自愈），代理开关走热重载
/// active/idle config，日常零提权。app 重启后 reconnect 已复用置 tun_active，此处命中分支 1。
async fn ensure_root_mihomo(
    app: &AppHandle,
    state: &ProxyState,
    params: &RunParams,
) -> Result<(), String> {
    let base = format!("http://127.0.0.1:{}", params.controller_port);
    // 1. 复用：controller 可达 + secret 匹配（reconnect 成功 / 本 session 已开过）
    if controller::check_auth(&base, &params.secret)
        .await
        .unwrap_or(false)
    {
        state.tun_active.store(true, Ordering::Relaxed);
        return Ok(());
    }
    // 2. plist 已装但进程未就绪（刚开机/崩溃重启中）：等 launchd 拉起
    if tun::plist_installed(app)
        && controller::wait_ready(&base, &params.secret, 12000)
            .await
            .is_ok()
    {
        state.tun_active.store(true, Ordering::Relaxed);
        return Ok(());
    }
    // plist 未装或等待超时（损坏/加载失败）→ 首次安装或重装
    // 3. 首次安装（提权一次）
    tun::install_launchdaemon(app, params).await?;
    state.tun_active.store(true, Ordering::Relaxed);
    Ok(())
}

/// 启动代理（统一 TUN 模式）。已开启视为成功（幂等）。
pub(crate) async fn start_core(
    app: &AppHandle,
    state: &ProxyState,
    params: RunParams,
) -> Result<(), String> {
    if state.enabled.load(Ordering::Relaxed) {
        return Ok(()); // 幂等：已开启
    }
    // 作废挂起的 TUN 释放重试（stop 的乐观后台路径）：重试按 release_gen 比对自弃，
    // 从源头消灭「重试 PUT idle 落在本轮 PUT active 之后」的竞态窗口（enabled 在
    // start_core 末尾才置位，靠它拦截存在缝隙）。
    state.release_gen.fetch_add(1, Ordering::Relaxed);
    // Pre-flight：TUN auto-route 路由已存在时 mihomo TUN 必然失败。占用者若是 Voidnix 对端变体
    // （dev/prod）残留的 active mihomo（app 退出后 launchd 继续托管），先经其 controller
    // 优雅让渡（热重载 idle 释放 TUN，免提权）再继续；第三方工具不可控，维持报错。
    if let Some(msg) = tun_route_conflict() {
        release_sibling_tun(app, msg).await?;
    }
    // 确保 root mihomo 在跑（launchd 托管：复用/等拉起/首次安装），安装后跑 idle config。
    // 统一热重载 active config 开启代理——install 后从 idle 切 active，复用时确认状态，免提权。
    ensure_root_mihomo(app, state, &params).await?;
    let log_before = log_size(app); // reload 前快照，供 verify_tun_active 区分新增行
    reload_config_yaml(app, &params).await?;
    // 同步 TUN 验证：PUT /configs 返回 204 不代表 TUN 创建成功（别的工具占路由时静默失败）。
    // 同步检测不阻塞 UX（200ms + reload < 500ms），且失败时即时回滚 idle config 清理 mihomo
    // 状态——避免遗留 broken active config 致 controller 逐渐无响应、后续重开走 osascript 重装。
    if let Err(e) = verify_tun_active(app, log_before).await {
        rollback_to_idle(app, &params).await;
        return Err(e);
    }
    *state.run_params.lock().map_err(|e| e.to_string())? = Some(params);
    state.enabled.store(true, Ordering::Relaxed);
    ensure_monitor(app); // 启动健康监测（幂等：已在跑则跳过）
    Ok(())
}

/// 停止代理（流量切直通）。乐观关闭——成功/后台重试均立即返回 Ok，UI 即时显示关闭。
///
/// controller 卡住时的容错策略（解决其他代理工具断开等网络风暴场景下关不掉的问题）：
/// 1. 先写 idle config.yaml 到磁盘（即使 API 卡住，mihomo 重启后自动加载 idle）
/// 2. 短超时（3s）单次尝试 API 热重载，成功则 TUN 即时释放
/// 3. 失败 + mihomo 已死 → 视为已关闭
/// 4. 失败 + mihomo 在跑 → 乐观返回 Ok，后台异步重试释放 TUN，全部失败才通知用户
pub(crate) async fn stop_core(app: &AppHandle, state: &ProxyState) -> Result<(), String> {
    invalidate_monitor(state);
    app.state::<StreamRegistry>().cancel_all();
    if state.tun_active.load(Ordering::Relaxed) {
        // 日志超限截断（低频点：用户主动关代理；mihomo 继续跑，O_APPEND 追加到新 EOF）
        if let Ok(dir) = crate::runtime::storage::ext_data_dir(app, "proxy") {
            truncate_log_if_large(&dir);
        }
        let idle = state.run_params.lock().map_err(|e| e.to_string())?.clone();
        if let Some(mut p) = idle {
            p.mode = "direct".into();
            p.tun = false;
            match write_run_config(app, &p) {
                Ok(path) => {
                    let base = format!("http://127.0.0.1:{}", p.controller_port);
                    let path_str = path.to_string_lossy().to_string();
                    // tun_active 语义 = root mihomo 进程是否在跑（非 TUN 设备是否占用），
                    // 成功/后台重试时进程仍常驻跑 idle config，故保持 true；仅进程退出才置 false。
                    match controller::reload_config(
                        &base,
                        &p.secret,
                        &path_str,
                        Duration::from_secs(3),
                        1,
                    )
                    .await
                    {
                        Ok(()) => {}
                        Err(_) if !root_mihomo_running(app) => {
                            // mihomo 已死：TUN 随进程退出释放。
                            state.tun_active.store(false, Ordering::Relaxed);
                        }
                        Err(_) => {
                            // controller 卡住但进程在跑：config.yaml 已写 idle，后台异步重试释放
                            // TUN，不阻塞用户关闭开关。active/idle 是独立文件，后台重试加载
                            // config.yaml（idle）与用户重开时写 config-active.yaml 不冲突。
                            let app2 = app.clone();
                            let secret = p.secret.clone();
                            // 捕获重试代际：start_core 入口自增后旧重试自弃（enabled 置位
                            // 前的缝隙期竞态由它消灭）
                            let retry_gen = state.release_gen.load(Ordering::Relaxed);
                            tauri::async_runtime::spawn(async move {
                                let st = app2.state::<ProxyState>();
                                for delay in [3u64, 6, 10, 15] {
                                    tokio::time::sleep(Duration::from_secs(delay)).await;
                                    // 用户已重新开代理（重开已开始或已完成）则停止释放尝试
                                    if st.release_gen.load(Ordering::Relaxed) != retry_gen
                                        || st.enabled.load(Ordering::Relaxed)
                                    {
                                        return;
                                    }
                                    if controller::reload_config(
                                        &base,
                                        &secret,
                                        &path_str,
                                        Duration::from_secs(3),
                                        1,
                                    )
                                    .await
                                    .is_ok()
                                    {
                                        return;
                                    }
                                }
                                let _ = app2.emit(
                                    "proxy-status",
                                    ProxyStatus {
                                        kind: "error".into(),
                                        msg: "代理关闭超时，TUN 可能仍占用，建议重新开启后再关闭"
                                            .into(),
                                    },
                                );
                            });
                        }
                    }
                }
                Err(_) => {
                    // 写盘失败（极端情况：磁盘满/权限）。config.yaml 残留上次 idle（永不含 active），
                    // 但无法保证内容与当前订阅一致，保守返回错误让用户感知。
                    if root_mihomo_running(app) {
                        return Err("关闭代理失败：无法写入配置文件".to_string());
                    }
                    state.tun_active.store(false, Ordering::Relaxed);
                }
            }
        }
    }
    state.enabled.store(false, Ordering::Relaxed);
    Ok(())
}

// ── 健康监测 + 自动热重载恢复 ──

/// 使在跑的健康监测 task 失效（代际自增，task 醒来比对失配即退出）。
/// stop_core / reset_dead_state 调用。
fn invalidate_monitor(state: &ProxyState) {
    state.monitor_gen.fetch_add(1, Ordering::Relaxed);
}

/// 启动健康监测 task（幂等：当前代际已有 task 则跳过）。start_core 成功后调用。
pub(crate) fn ensure_monitor(app: &AppHandle) {
    let state = app.state::<ProxyState>();
    let cur = state.monitor_gen.load(Ordering::Relaxed);
    if state.monitor_spawned_gen.load(Ordering::Relaxed) == cur {
        return; // 当前代际已在跑
    }
    state.monitor_spawned_gen.store(cur, Ordering::Relaxed);
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        health_monitor(&app2, cur).await;
    });
}

/// 运行 config 的 tun.enable 是否已被关闭（enabled 前提下的状态脱节判定）。
/// 读失败/字段缺失返回 false（不动作），不误判。
async fn tun_disabled(base: &str, secret: &str) -> bool {
    controller::get_configs(base, secret)
        .await
        .ok()
        .and_then(|v| {
            v.get("tun")
                .and_then(|t| t.get("enable"))
                .and_then(Value::as_bool)
        })
        == Some(false)
}

/// 健康监测主循环（每 30s 一轮）。代际失配即退出（stop/reset 作废，见 `invalidate_monitor`）。
///
/// 单频两职：先做**不变式对账**（enabled ⇒ tun.enable，违例即复位——覆盖无通知推送的
/// 脱节路径：核心崩溃后 KeepAlive 重启进 idle、外部持 secret 改动配置；让渡路径有分布式
/// 通知即时对账，此处仅兜底），再跑**出站探针**（controller 可达 + 主节点 delay test，
/// 连续 2 轮异常才恢复动作）。
async fn health_monitor(app: &AppHandle, gen: u64) {
    let state = app.state::<ProxyState>();
    let mut fail_streak = 0u32;
    let mut notified_error = false;
    while state.monitor_gen.load(Ordering::Relaxed) == gen {
        tokio::time::sleep(Duration::from_secs(30)).await;
        if state.monitor_gen.load(Ordering::Relaxed) != gen {
            break;
        }
        if !state.enabled.load(Ordering::Relaxed) {
            continue; // 未启用不监测
        }
        let Some(p) = state.run_params.lock().ok().and_then(|g| g.clone()) else {
            continue;
        };
        let base = format!("http://127.0.0.1:{}", p.controller_port);

        // 不变式对账：enabled 但 tun.enable=false = 状态脱节（核心重启回退 idle / 外部
        // 改动 / 让渡未及通知），流量实际直通。controller/测速探针对此无感（idle config
        // 仍含全部节点），半路由也无感（接管后路由仍在只是换了主人）——读运行 config 才能识别。
        if tun_disabled(&base, &p.secret).await {
            reset_dead_state(app, "代理已断开（TUN 未生效），请重新开启");
            break;
        }

        // 出站探针 + 异常恢复。
        if probe_health(&base, &p.secret).await {
            if notified_error {
                let _ = app.emit(
                    "proxy-status",
                    ProxyStatus {
                        kind: "success".into(),
                        msg: "代理已恢复".into(),
                    },
                );
                notified_error = false;
            }
            fail_streak = 0;
            continue;
        }
        fail_streak += 1;
        if fail_streak < 2 {
            continue; // 容忍单次抖动，连续 2 轮异常才动作
        }
        fail_streak = 0;

        let ctrl_ok = controller::check_auth(&base, &p.secret)
            .await
            .unwrap_or(false);
        let alive = root_mihomo_running(app);
        if !alive || !ctrl_ok {
            reset_dead_state(app, "代理核心异常退出，请重新开启");
            break;
        }
        if state.monitor_gen.load(Ordering::Relaxed) != gen
            || !state.enabled.load(Ordering::Relaxed)
        {
            break;
        }
        let mut active = p;
        active.tun = true;
        if let Err(e) = reload_config_yaml(app, &active).await {
            notified_error = true;
            let _ = app.emit(
                "proxy-status",
                ProxyStatus {
                    kind: "error".into(),
                    msg: format!("代理出站异常，自动恢复失败：{e}"),
                },
            );
        }
    }
}

/// 健康探针：controller 可达（GET /version）+ 当前主节点出站可达（delay test）。
async fn probe_health(base: &str, secret: &str) -> bool {
    if !controller::check_auth(base, secret).await.unwrap_or(false) {
        return false;
    }
    let Ok(val) = controller::get_proxies(base, secret).await else {
        return false;
    };
    match parse_current_node(&val) {
        Some(node) => {
            controller::test_delay(base, secret, &node)
                .await
                .unwrap_or(0)
                > 0
        }
        None => true,
    }
}

/// 进程已退出/不可控：重置内存状态 + 通知前端 + 清理残留 pidfile + 停监测（代际失效）。
fn reset_dead_state(app: &AppHandle, msg: &str) {
    let state = app.state::<ProxyState>();
    state.enabled.store(false, Ordering::Relaxed);
    state.tun_active.store(false, Ordering::Relaxed);
    invalidate_monitor(&state);
    app.state::<StreamRegistry>().cancel_all();
    let _ = app.emit("proxy-enabled", false);
    let _ = app.emit(
        "proxy-status",
        ProxyStatus {
            kind: "error".into(),
            msg: msg.to_string(),
        },
    );
    crate::runtime::menubar::refresh(app);
}

/// 核心运行中时热重载 active config 以应用配置变更（订阅增删）。
/// 仅 `reload_running_config` 内部调用（enabled 场景委托）。
async fn reload_if_running(app: &AppHandle, state: &ProxyState) -> Result<(), String> {
    if !state.enabled.load(Ordering::Relaxed) {
        return Ok(());
    }
    let params = state
        .run_params
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "core running but no run params".to_string())?;

    reload_config_yaml(app, &params).await?;
    Ok(())
}

/// 重载当前运行配置（按 enabled/tun_active 自适应 active 或 idle）。
/// 激活订阅切换等需在任意运行态（含 idle 常驻）刷新节点列表的场景使用：
/// enabled → active config；idle 常驻（tun_active 但未启用）→ idle config；
/// 进程未运行 → no-op。run_params.active_sub_id 须在调用前已更新。
pub(crate) async fn reload_running_config(
    app: &AppHandle,
    state: &ProxyState,
) -> Result<(), String> {
    if state.enabled.load(Ordering::Relaxed) {
        return reload_if_running(app, state).await;
    }
    if !state.tun_active.load(Ordering::Relaxed) {
        return Ok(()); // 进程未运行，无可重载
    }
    // idle 常驻：run_params 持最近 active 参数，派生 idle（direct + 无 tun）重载
    let Some(mut p) = state.run_params.lock().map_err(|e| e.to_string())?.clone() else {
        return Ok(());
    };
    p.mode = "direct".into();
    p.tun = false;
    reload_config_yaml(app, &p).await
}

/// 取 mihomo controller endpoint（base URL + secret），代理未开启时报错。
pub(crate) fn controller_endpoint(state: &ProxyState) -> Result<(String, String), String> {
    let params = state
        .run_params
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "代理未开启".to_string())?;
    Ok((
        format!("http://127.0.0.1:{}", params.controller_port),
        params.secret,
    ))
}

/// 读 controller 端口 + secret。mihomo 未运行返回 None。
pub(crate) fn controller_creds_opt(state: &ProxyState) -> Option<(u16, String)> {
    state
        .run_params
        .lock()
        .ok()?
        .as_ref()
        .map(|p| (p.controller_port, p.secret.clone()))
}

/// 解析 /proxies 响应取主分组当前选中节点名。镜像 logic.ts pickMainGroup 语义。
pub(crate) fn parse_current_node(val: &Value) -> Option<String> {
    let proxies = val.get("proxies").and_then(|p| p.as_object())?;
    let main = proxies
        .iter()
        .find(|(k, e)| {
            e.get("type").and_then(|t| t.as_str()) == Some("Selector") && k.as_str() != "GLOBAL"
        })
        .map(|(_, v)| v)?;
    main.get("now").and_then(|n| n.as_str()).map(String::from)
}

/// app 启动复用上次退出遗留的 root mihomo（常驻方案下进程在 app 退出后仍跑）。
/// 运行 config 仍 active（tun.enable=true）则恢复 enabled 并 emit 同步 UI/菜单；idle 则
/// 幂等复位直通。secret 不匹配/不可达不提权清理（避免启动弹窗），下次开代理 install 接管。
pub(crate) async fn reconnect_root_mihomo(app: &AppHandle) {
    if !root_mihomo_running(app) {
        return;
    }
    let state = app.state::<ProxyState>();
    // 一次读取构造完整 RunParams（含激活订阅 id），供 idle 热重载复用。
    let Some(p) = read_run_params(app) else {
        crate::runtime::menubar::refresh(app);
        return;
    };
    let base = format!("http://127.0.0.1:{}", p.controller_port);
    match controller::check_auth(&base, &p.secret).await {
        Ok(false) => {
            // secret 不匹配（旧 osascript 残留/不可控实例）：不提权清理（避免 app 启动弹窗），
            // 下次开代理时 install_launchdaemon 会清理自己的旧 mihomo 并重新接管。
            log::debug!("[proxy] 残留 mihomo secret 不匹配，跳过复用（下次开代理时清理接管）");
        }
        Ok(true) => {
            // 运行 config 的 tun.enable 判定实际状态。active（app 退出前开着、launchd 保活
            // 至今）→ 恢复 enabled + emit 同步前端/菜单 + 重启监测：常驻设计意图是 app 退出
            // 不影响代理，重启 app 静默切直连等于无提示的流量裸奔。idle → 热重载 idle 幂等
            // 复位直通（config.yaml 语义不变）。
            let live = controller::get_configs(&base, &p.secret).await.ok();
            let tun_on = live
                .as_ref()
                .and_then(|v| v.get("tun"))
                .and_then(|t| t.get("enable"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if tun_on {
                let mut active = p.clone();
                active.tun = true;
                // mode 以运行 config 为准（权威），读取失败回退 config.json 持久值
                if let Some(mode) = live
                    .as_ref()
                    .and_then(|v| v.get("mode"))
                    .and_then(Value::as_str)
                {
                    active.mode = mode.to_string();
                }
                if let Ok(mut g) = state.run_params.lock() {
                    *g = Some(active);
                }
                state.enabled.store(true, Ordering::Relaxed);
                state.tun_active.store(true, Ordering::Relaxed);
                ensure_monitor(app);
                let _ = app.emit("proxy-enabled", true);
                // 拉取当前节点名填充菜单状态行（与 set_proxy_enabled 开启路径对齐）
                let app2 = app.clone();
                tauri::async_runtime::spawn(async move {
                    super::menu::refresh_proxy_menu(&app2).await;
                });
            } else {
                let idle = RunParams {
                    mixed_port: p.mixed_port,
                    controller_port: p.controller_port,
                    secret: p.secret,
                    mode: "direct".into(),
                    active_sub_id: p.active_sub_id,
                    tun: false,
                };
                match reload_config_yaml(app, &idle).await {
                    Ok(()) => {
                        state.tun_active.store(true, Ordering::Relaxed);
                        if let Ok(mut g) = state.run_params.lock() {
                            *g = Some(idle);
                        }
                    }
                    Err(e) => log::debug!("[proxy] 复用残留 mihomo、热重载 idle 失败: {e}"),
                }
            }
        }
        Err(e) => log::debug!("[proxy] 残留 mihomo controller 不可达，跳过复用: {e}"),
    }
    crate::runtime::menubar::refresh(app);
}

/// 按 mihomo binary 完整路径 ps 查指定数据目录是否有实例在跑（本端/对端共用）。
/// 匹配 = 命令行以 binary 完整路径开头 + 边界（空格/行尾）——launchd 以绝对路径启动，
/// args 首段即 binary 路径。子串包含会把 `tail -f <dir>/mihomo.log` 等命令行误判为在跑。
fn mihomo_running(dir: &Path) -> bool {
    let bin = dir.join("mihomo").display().to_string();
    let Ok(out) = std::process::Command::new("ps")
        .args(["-eo", "args"])
        .output()
    else {
        return false;
    };
    String::from_utf8_lossy(&out.stdout).lines().any(|l| {
        l.strip_prefix(bin.as_str())
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
    })
}

/// 按 mihomo binary 完整路径 ps 查是否有 root 实例在跑。
pub(crate) fn root_mihomo_running(app: &AppHandle) -> bool {
    crate::runtime::storage::ext_data_dir(app, "proxy")
        .map(|d| mihomo_running(&d))
        .unwrap_or(false)
}

/// 读 extensions/proxy/config.json 构造 RunParams。
fn read_run_params(app: &AppHandle) -> Option<RunParams> {
    let path = crate::runtime::storage::ext_data_dir(app, "proxy")
        .ok()?
        .join("config.json");
    let text = std::fs::read_to_string(&path).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    let mut mixed_port = v.get("mixedPort")?.as_u64()? as u16;
    let mut controller_port = v.get("controllerPort")?.as_u64()? as u16;
    // 端口变体归一化：config.json 可能残留对端变体端口，reconnect 前修正
    core::correct_variant_ports(&mut mixed_port, &mut controller_port);
    Some(RunParams {
        mixed_port,
        controller_port,
        secret: v.get("secret")?.as_str()?.to_string(),
        mode: v
            .get("mode")
            .and_then(|m| m.as_str())
            .unwrap_or("rule")
            .to_string(),
        active_sub_id: v
            .get("activeSubscriptionId")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string(),
        tun: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sibling_identifier_swaps_dev_suffix() {
        // prod → dev：加后缀
        assert_eq!(
            sibling_identifier("com.litiantao.voidnix", false),
            Some("com.litiantao.voidnix.dev".into())
        );
        // dev → prod：去后缀
        assert_eq!(
            sibling_identifier("com.litiantao.voidnix.dev", true),
            Some("com.litiantao.voidnix".into())
        );
        // dev 判定但 identifier 缺 .dev 后缀（配置异常）：无法推导对端，返回 None
        assert_eq!(sibling_identifier("com.litiantao.voidnix", true), None);
    }

    #[test]
    fn read_log_tail_reads_new_complete_lines_since_offset() {
        let dir = std::env::temp_dir().join(format!("voidnix-logtail-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("mihomo.log");
        std::fs::write(&log, "old1\nold2\n").unwrap();
        let since = std::fs::metadata(&log).unwrap().len();

        // since 后无换行（行未写完）→ 无完整新增行
        std::fs::write(&log, "old1\nold2\npartial-no-newline").unwrap();
        assert!(read_log_tail(&log, since).is_empty());

        // 首个行段丢弃（快照时可能未写完），取其后完整行——与旧 tail_after 语义一致
        std::fs::write(&log, "old1\nold2\npartial-no-newline\n[TUN] error\nlater\n").unwrap();
        assert_eq!(
            read_log_tail(&log, since),
            vec!["[TUN] error".to_string(), "later".to_string()]
        );

        // 文件被截断/轮转（len <= since）→ 无新增
        std::fs::write(&log, "x\n").unwrap();
        assert!(read_log_tail(&log, since).is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_log_tail_degrades_to_window_when_oversized() {
        let dir = std::env::temp_dir().join(format!("voidnix-logtail2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("mihomo.log");
        let mut content = String::new();
        let filler = "x".repeat(96);
        for i in 0..3000 {
            content.push_str(&format!("{i:05} {filler}\n")); // ~102B/行，共 ~300KB
        }
        std::fs::write(&log, &content).unwrap();
        // since=0 且体积远超 64KB 窗口 → 只取尾部窗口内的完整行（首段丢弃），末行完整保留
        let lines = read_log_tail(&log, 0);
        let last = content.lines().next_back().unwrap().to_string();
        assert_eq!(lines.last().unwrap(), &last);
        assert!(lines.len() > 0 && lines.len() < 1000);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn has_tun_routes_detects_both_generations() {
        // 老版半路由（0.0.0.0/1 + 128.0.0.0/1）
        assert!(has_tun_routes("0/1  10.255.255.1  UGSc  utun4\n"));
        assert!(has_tun_routes("128/1  10.255.255.1  UGSc  utun4\n"));
        // 新版路由树分解（1 + 2/7 + … + 128.0/1，避开 0.0.0.0/8）
        let modern = "default  192.168.31.1  UGScg  en0\n\
                      1  198.18.0.1  UGSc  utun4\n\
                      2/7  198.18.0.1  UGSc  utun4\n\
                      4/6  198.18.0.1  UGSc  utun4\n\
                      8/5  198.18.0.1  UGSc  utun4\n\
                      16/4  198.18.0.1  UGSc  utun4\n\
                      32/3  198.18.0.1  UGSc  utun4\n\
                      64/2  198.18.0.1  UGSc  utun4\n\
                      128.0/1  198.18.0.1  UGSc  utun4\n";
        assert!(has_tun_routes(modern));
        // 普通路由不匹配（default / 回环 127 / 链路本地 169.254 / 子网 / IPv6 分解树）
        assert!(!has_tun_routes(
            "default  192.168.1.1  UGSc  en0\n115.28/16  link#4  UCS  en0\n\
             127  127.0.0.1  UCS  lo0\n169.254  link#14  UCS  en0\n\
             100::/8  fdfe:dcba:9876::1  UGSc  utun4\n"
        ));
        // 空表
        assert!(!has_tun_routes(""));
    }
}
