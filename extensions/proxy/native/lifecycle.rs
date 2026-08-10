//! 代理核心生命周期：ProxyState + root mihomo 启停/热重载/健康监测/启动复用。
//! 命令入口见 mod.rs；菜单栏见 menu.rs。

use super::controller;
use super::core::{self, RunParams};
use super::stream::StreamRegistry;
use super::subscription;
use super::tun;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

/// 代理运行状态：enabled（流量是否被代理）+ tun_active（root mihomo 进程是否在跑，常驻）
/// + run_params（最近一次 active 参数，供热重载/复用）+ monitor_alive（健康监测 task 运行标志）。
pub struct ProxyState {
    pub enabled: AtomicBool,
    pub run_params: Mutex<Option<RunParams>>,
    pub tun_active: AtomicBool,
    /// 菜单栏状态行当前节点名缓存；由 refresh_proxy_menu 异步拉取填充。
    pub current_node: Mutex<Option<String>>,
    /// 健康监测 task 运行标志（协作式退出）：true=监测中。start_core 置 true 并 spawn；
    /// stop_core / 进程退出重置时置 false，task 自行退出。
    pub monitor_alive: AtomicBool,
}

/// 健康事件 payload（emit "proxy-status"，前端 showStatus 反馈）。
#[derive(Clone, serde::Serialize)]
pub(crate) struct ProxyStatus {
    /// "success" | "error"：对齐 StatusBar kind 语义，错误必须 error 避免绿色对勾错位。
    pub kind: String,
    pub msg: String,
}

/// 写 config.yaml + PUT /configs 热重载（active/idle 切换、订阅变更共用）。
/// TUN 模式下 root mihomo 常驻，代理开关 = 热重载 active/idle config，免 spawn 免提权。
pub(crate) async fn reload_config_yaml(app: &AppHandle, params: &RunParams) -> Result<(), String> {
    let yaml = subscription::build_run_config(app, params)?;
    let path = core::run_config_path(app)?;
    std::fs::write(&path, yaml).map_err(|e| e.to_string())?;
    let base = format!("http://127.0.0.1:{}", params.controller_port);
    controller::reload_config(&base, &params.secret, &path.to_string_lossy()).await
}

/// 读 mihomo.log 当前字节大小。reload 前快照，供 verify_tun_active 区分新增行。
pub(crate) fn log_size(app: &AppHandle) -> u64 {
    crate::runtime::storage::ext_data_dir(app, "proxy")
        .ok()
        .and_then(|d| std::fs::metadata(d.join("mihomo.log")).ok())
        .map(|m| m.len())
        .unwrap_or(0)
}

/// 从 `since` 字节偏移之后取新增内容（跳到下一个换行确保完整行）。
/// `\n` 是 ASCII 单字节，在 UTF-8 字节流中直接搜索不与多字节字符冲突；
/// 换行后的位置天然是字符边界，`&content[pos..]` 不会 panic。
fn tail_after(content: &str, since: u64) -> &str {
    let start = (since as usize).min(content.len());
    match content.as_bytes()[start..].iter().position(|&b| b == b'\n') {
        Some(i) => &content[start + i + 1..],
        None => "", // since 之后无换行 = 无新增完整行
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
/// buffer，非等待 mihomo 处理。start_core 中后台调用不阻塞 enabled 翻转。
pub(crate) async fn verify_tun_active(app: &AppHandle, since: u64) -> Result<(), String> {
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let Ok(dir) = crate::runtime::storage::ext_data_dir(app, "proxy") else {
        return Ok(()); // 路径异常不阻塞（reload 已成功为前提）
    };
    let content = std::fs::read_to_string(dir.join("mihomo.log")).unwrap_or_default();
    for line in tail_after(&content, since).lines().rev().take(15) {
        let l = line.to_lowercase();
        // 正常日志 "[TUN] Tun adapter listening at: utun4" 无 error 关键词不会误报
        if l.contains("tun") && (l.contains("error") || l.contains("exist")) {
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
    // 确保 root mihomo 在跑（launchd 托管：复用/等拉起/首次安装），安装后跑 idle config。
    // 统一热重载 active config 开启代理——install 后从 idle 切 active，复用时确认状态，免提权。
    ensure_root_mihomo(app, state, &params).await?;
    let log_before = log_size(app); // reload 前快照，供 verify_tun_active 区分新增行
    reload_config_yaml(app, &params).await?;
    // TUN 静默失效检测后台化：PUT /configs 同步完成 TUN 创建（实测返回时日志已写入），
    // verify 读日志检测静默失败（别的工具占 TUN 时 mihomo API 仍 204）。后台化不阻塞
    // enabled 翻转——正常路径零感知，仅 TUN 被占（极少）时经事件回滚 enabled + toast。
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = verify_tun_active(&app2, log_before).await {
            let st = app2.state::<ProxyState>();
            // 竞态保护：用户可能在 verify 期间关闭重开，仅当前仍 enabled 才回滚
            if st.enabled.swap(false, Ordering::Relaxed) {
                let _ = app2.emit("proxy-enabled", false);
                let _ = app2.emit(
                    "proxy-status",
                    ProxyStatus {
                        kind: "error".into(),
                        msg: e,
                    },
                );
                crate::runtime::menubar::refresh(&app2);
            }
        }
    });
    *state.run_params.lock().map_err(|e| e.to_string())? = Some(params);
    state.enabled.store(true, Ordering::Relaxed);
    ensure_monitor(app); // 启动健康监测（幂等：已在跑则跳过）
    Ok(())
}

/// 停止代理（流量切直通）。
pub(crate) async fn stop_core(app: &AppHandle, state: &ProxyState) -> Result<(), String> {
    state.monitor_alive.store(false, Ordering::Relaxed); // 用户主动关闭，停健康监测
    app.state::<StreamRegistry>().cancel_all(); // 停所有 WS 流（traffic/connections/logs）
    if state.tun_active.load(Ordering::Relaxed) {
        let idle = state.run_params.lock().map_err(|e| e.to_string())?.clone();
        if let Some(mut p) = idle {
            p.mode = "direct".into();
            p.tun = false;
            // 热重载 idle 撤销 TUN、流量直通（mihomo 进程保留，launchd 继续托管）。
            if let Err(e) = reload_config_yaml(app, &p).await {
                // controller 不可达时区分两种情况：
                // - mihomo 在但 controller 卡死 → 无法热重载也无法杀（KeepAlive 会拉起），返回 Err
                //   让前端 catch 通知用户（不 emit proxy-status 避免与 catch 双重 toast）
                if root_mihomo_running(app) {
                    return Err(format!("关闭代理失败：{e}"));
                }
                // - mihomo 已死（崩溃/launchd 未拉起）→ 代理本就不在跑，视为已关闭
                state.tun_active.store(false, Ordering::Relaxed);
            }
        }
    }
    state.enabled.store(false, Ordering::Relaxed);
    Ok(())
}

// ── 健康监测 + 自动热重载恢复 ──

/// 启动健康监测 task（幂等：已在跑则跳过）。start_core 成功后调用。
pub(crate) fn ensure_monitor(app: &AppHandle) {
    let state = app.state::<ProxyState>();
    if state.monitor_alive.swap(true, Ordering::Relaxed) {
        return; // 已在跑
    }
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        health_monitor(&app2).await;
    });
}

/// 健康监测主循环。协作式退出（monitor_alive=false 即停）。
async fn health_monitor(app: &AppHandle) {
    let state = app.state::<ProxyState>();
    let mut fail_streak = 0u32;
    let mut notified_error = false;
    while state.monitor_alive.load(Ordering::Relaxed) {
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        if !state.monitor_alive.load(Ordering::Relaxed) {
            break;
        }
        if !state.enabled.load(Ordering::Relaxed) {
            continue; // 未启用不监测
        }
        let Some(p) = state.run_params.lock().ok().and_then(|g| g.clone()) else {
            continue;
        };
        let base = format!("http://127.0.0.1:{}", p.controller_port);

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
        if !state.monitor_alive.load(Ordering::Relaxed) || !state.enabled.load(Ordering::Relaxed) {
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
    state.monitor_alive.store(false, Ordering::Relaxed);
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

/// 进程已退出/不可控：重置内存状态 + 通知前端 + 清理残留 pidfile + 停监测。
fn reset_dead_state(app: &AppHandle, msg: &str) {
    let state = app.state::<ProxyState>();
    state.enabled.store(false, Ordering::Relaxed);
    state.tun_active.store(false, Ordering::Relaxed);
    state.monitor_alive.store(false, Ordering::Relaxed);
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
        Err(e) => log::debug!("[proxy] 残留 mihomo controller 不可达，跳过复用: {e}"),
    }
    crate::runtime::menubar::refresh(app);
}

/// 按 mihomo binary 完整路径 ps 查是否有 root 实例在跑。
pub(crate) fn root_mihomo_running(app: &AppHandle) -> bool {
    let Ok(dir) = crate::runtime::storage::ext_data_dir(app, "proxy") else {
        return false;
    };
    let bin = dir.join("mihomo").display().to_string();
    let Ok(out) = std::process::Command::new("ps")
        .args(["-eo", "args"])
        .output()
    else {
        return false;
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|l| l.contains(&bin))
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
