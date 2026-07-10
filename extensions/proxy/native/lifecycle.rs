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

/// restart_root + wait_ready。失败不清理（restart_root 含杀旧逻辑，下次重启自动回收）。
async fn restart_and_wait(app: &AppHandle, params: &RunParams) -> Result<(), String> {
    let base = format!("http://127.0.0.1:{}", params.controller_port);
    tun::restart_root(app, params).await?;
    controller::wait_ready(&base, &params.secret, 8000).await
}

/// 确保 root mihomo 进程在跑（常驻）。已常驻且状态可信则幂等返回；否则 restart_root
/// （单次提权）并 wait_ready。一旦启动，进程常驻到显式 stop_root 或 app 退出——
/// 代理开关与 TUN 切换均走热重载，免再提权。
///
/// 返回 `true` 表示本次新 spawn（提权一次），`false` 表示复用已常驻进程。
async fn ensure_root_mihomo(
    app: &AppHandle,
    state: &ProxyState,
    params: &RunParams,
) -> Result<bool, String> {
    if state.tun_active.load(Ordering::Relaxed) {
        return Ok(false); // 本 session 已知状态，可热重载复用
    }
    restart_and_wait(app, params).await?;
    state.tun_active.store(true, Ordering::Relaxed);
    Ok(true)
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
    let freshly_spawned = ensure_root_mihomo(app, state, &params).await?;
    if !freshly_spawned {
        // 复用常驻进程：热重载 active config 恢复代理。失败则单次提权重启。
        if let Err(reload_err) = reload_config_yaml(app, &params).await {
            eprintln!("[proxy] 复用进程热重载失败，回退重启: {reload_err}");
            state.tun_active.store(false, Ordering::Relaxed);
            restart_and_wait(app, &params).await?;
            state.tun_active.store(true, Ordering::Relaxed);
        }
    }
    *state.run_params.lock().map_err(|e| e.to_string())? = Some(params);
    state.enabled.store(true, Ordering::Relaxed);
    ensure_monitor(app); // 启动健康监测（幂等：已在跑则跳过）
    Ok(())
}

/// 停止代理（流量切直通）。
pub(crate) async fn stop_core(app: &AppHandle, state: &ProxyState) -> Result<(), String> {
    state.monitor_alive.store(false, Ordering::Relaxed); // 用户主动关闭，停健康监测
                                                         // 停所有 WS 流（traffic/connections/logs）
    app.state::<StreamRegistry>().cancel_all();
    if state.tun_active.load(Ordering::Relaxed) {
        let idle = state.run_params.lock().map_err(|e| e.to_string())?.clone();
        if let Some(mut p) = idle {
            p.mode = "direct".into();
            p.tun = false;
            if let Err(e) = reload_config_yaml(app, &p).await {
                if root_mihomo_running(app) {
                    tun::stop_root(app)?;
                }
                state.tun_active.store(false, Ordering::Relaxed);
                eprintln!("[proxy] 热重载 idle 失败，回退强杀关闭: {e}");
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
    if let Ok(dir) = crate::runtime::storage::ext_data_dir(app, "proxy") {
        let _ = std::fs::remove_file(dir.join("mihomo.pid"));
    }
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

/// 核心运行中时热重载以应用配置变更（订阅增删）。
pub(crate) async fn reload_if_running(app: &AppHandle, state: &ProxyState) -> Result<(), String> {
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
    let Some((mixed_port, controller_port, secret)) = read_controller_creds(app) else {
        crate::runtime::menubar::refresh(app);
        return;
    };
    let base = format!("http://127.0.0.1:{controller_port}");
    match controller::check_auth(&base, &secret).await {
        Ok(false) => {
            if let Err(e) = tun::stop_root(app) {
                eprintln!("[proxy] 清理 secret 不一致残留 mihomo 失败: {e}");
            }
        }
        Ok(true) => {
            let idle = RunParams {
                mixed_port,
                controller_port,
                secret,
                mode: "direct".into(),
                tun: false,
            };
            match reload_config_yaml(app, &idle).await {
                Ok(()) => {
                    state.tun_active.store(true, Ordering::Relaxed);
                    if let Ok(mut g) = state.run_params.lock() {
                        *g = Some(idle);
                    }
                }
                Err(e) => eprintln!("[proxy] 复用残留 mihomo、热重载 idle 失败: {e}"),
            }
        }
        Err(e) => eprintln!("[proxy] 残留 mihomo controller 不可达，跳过复用: {e}"),
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
    Some(RunParams {
        mixed_port: v.get("mixedPort")?.as_u64()? as u16,
        controller_port: v.get("controllerPort")?.as_u64()? as u16,
        secret: v.get("secret")?.as_str()?.to_string(),
        mode: v
            .get("mode")
            .and_then(|m| m.as_str())
            .unwrap_or("rule")
            .to_string(),
        tun: true,
    })
}

/// 读 config.json 的 controller 凭据。
fn read_controller_creds(app: &AppHandle) -> Option<(u16, u16, String)> {
    let p = read_run_params(app)?;
    Some((p.mixed_port, p.controller_port, p.secret))
}
