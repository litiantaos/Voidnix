//! 代理扩展：命令入口 + 生命周期装配。
//! 核心状态机见 lifecycle；菜单栏见 menu；mihomo 进程/订阅/流见 core/tun/subscription/stream。

mod controller;
mod core;
mod lifecycle;
mod menu;
mod stream;
mod subscription;
mod tun;

use crate::runtime::registry::Extension;
use serde::Serialize;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::task::JoinSet;

use self::core::RunParams;
use self::lifecycle::{
    controller_creds_opt, controller_endpoint, ensure_monitor, reload_config_yaml,
    reload_running_config, root_mihomo_running, start_core, stop_core, ProxyState,
};
use self::stream::{LogFrame, StreamRegistry, TrafficFrame};

/// 启用/停用代理（统一 TUN 模式：root mihomo 常驻 + 热重载 active/idle）。
/// 参数聚合前端 config 全部运行字段（端口/密钥/模式/激活订阅），Tauri 命令边界天然多参。
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn set_proxy_enabled(
    app: AppHandle,
    state: State<'_, ProxyState>,
    enabled: bool,
    mut mixed_port: u16,
    mut controller_port: u16,
    secret: String,
    mode: String,
    active_sub_id: String,
) -> Result<bool, String> {
    // 端口变体归一化（权威修正）：config.json 可能残留对端变体默认端口，
    // 在命令入口处静默修正，确保 mihomo 绑定到正确端口。
    core::correct_variant_ports(&mut mixed_port, &mut controller_port);
    if enabled {
        let params = RunParams {
            mixed_port,
            controller_port,
            secret,
            mode,
            active_sub_id,
            tun: true, // 统一 TUN 模式：active config 恒含 tun 段
        };
        start_core(&app, &state, params).await?;
        crate::runtime::menubar::refresh(&app);
        let _ = app.emit("proxy-enabled", true);
        let app2 = app.clone();
        tauri::async_runtime::spawn(async move {
            menu::refresh_proxy_menu(&app2).await;
        });
        Ok(true)
    } else {
        stop_core(&app, &state).await?;
        crate::runtime::menubar::refresh(&app);
        let _ = app.emit("proxy-enabled", false);
        Ok(false)
    }
}

#[tauri::command]
pub async fn is_proxy_enabled(state: State<'_, ProxyState>) -> Result<bool, String> {
    Ok(state.enabled.load(Ordering::Relaxed))
}

/// 查询核心状态（已下载/版本号/下载中），供列表「核心」项展示。
#[tauri::command]
pub async fn proxy_core_status(app: AppHandle) -> Result<core::CoreStatus, String> {
    Ok(core::core_status(&app))
}

/// 强制触发核心下载（未下载时）；前端「核心」项下载按钮调用。
#[tauri::command]
pub async fn proxy_ensure_core(app: AppHandle) -> Result<bool, String> {
    core::ensure_bin(&app).await?;
    Ok(true)
}

/// 检查更新：拉 GitHub API latest 版本 → 比对本地版本。API 不可达时静默 has_update=false。
#[tauri::command]
pub async fn proxy_check_update(app: AppHandle) -> Result<core::UpdateInfo, String> {
    Ok(core::check_update(&app).await)
}

/// 更新核心：停代理 → 卸载 LaunchDaemon → 删旧 binary → ensure_bin 重下最新 → 恢复。
#[tauri::command]
pub async fn proxy_update_core(app: AppHandle, state: State<'_, ProxyState>) -> Result<(), String> {
    let was_enabled = state.enabled.load(Ordering::Relaxed);
    let params = state.run_params.lock().map_err(|e| e.to_string())?.clone();
    if state.tun_active.load(Ordering::Relaxed) {
        tun::uninstall_launchdaemon(&app).await?;
        state.tun_active.store(false, Ordering::Relaxed);
    }
    state.enabled.store(false, Ordering::Relaxed);
    core::remove_core_files(&app)?;
    core::ensure_bin(&app).await?;
    if was_enabled {
        if let Some(p) = params {
            start_core(&app, &state, p).await?; // 重新 install_launchdaemon（提权）
        }
    }
    Ok(())
}

/// 拉取订阅并持久化（subs/<id>.yaml），返回节点数；核心运行中则热重载。
#[tauri::command]
pub async fn proxy_update_subscription(
    app: AppHandle,
    state: State<'_, ProxyState>,
    id: String,
    url: String,
) -> Result<usize, String> {
    let (count, text) = subscription::fetch(&url).await?;
    subscription::save(&app, &id, &text)?;
    reload_running_config(&app, &state).await?;
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        menu::refresh_proxy_menu(&app2).await;
    });
    Ok(count)
}

/// 删除订阅持久化文件；核心运行中则热重载。
/// `new_active_sub_id`：删除后新的激活订阅 id（删的若非激活则与当前一致），
/// 在热重载前写入 run_params，使 build_run_config 用新激活订阅重建 config。
#[tauri::command]
pub async fn proxy_remove_subscription(
    app: AppHandle,
    state: State<'_, ProxyState>,
    id: String,
    new_active_sub_id: String,
) -> Result<(), String> {
    subscription::remove(&app, &id)?;
    if let Ok(mut guard) = state.run_params.lock() {
        if let Some(p) = guard.as_mut() {
            p.active_sub_id = new_active_sub_id;
        }
    }
    reload_running_config(&app, &state).await?;
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        menu::refresh_proxy_menu(&app2).await;
    });
    Ok(())
}

/// 切换激活订阅：更新 run_params.active_sub_id + 热重载（核心运行中，含 idle 常驻）。
/// 仅激活订阅的节点参与合并，切换即重建 mihomo config（节点列表随之变更）。
#[tauri::command]
pub async fn proxy_set_active_subscription(
    app: AppHandle,
    state: State<'_, ProxyState>,
    id: String,
) -> Result<(), String> {
    if let Ok(mut guard) = state.run_params.lock() {
        if let Some(p) = guard.as_mut() {
            p.active_sub_id = id;
        }
    }
    reload_running_config(&app, &state).await?;
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        menu::refresh_proxy_menu(&app2).await;
    });
    Ok(())
}

/// GET /proxies → 完整代理树。未开代理时返回空。
#[tauri::command]
pub async fn proxy_get_proxies(state: State<'_, ProxyState>) -> Result<Value, String> {
    let Some((port, secret)) = controller_creds_opt(&state) else {
        return Ok(serde_json::json!({ "proxies": {} }));
    };
    let base = format!("http://127.0.0.1:{port}");
    controller::get_proxies(&base, &secret).await
}

/// PUT /proxies/{group} → 在 selector 分组选择节点。
#[tauri::command]
pub async fn proxy_select_proxy(
    app: AppHandle,
    state: State<'_, ProxyState>,
    group: String,
    name: String,
) -> Result<(), String> {
    let (base, secret) = controller_endpoint(&state)?;
    controller::select_proxy(&base, &secret, &group, &name).await?;
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        menu::refresh_proxy_menu(&app2).await;
    });
    Ok(())
}

/// 流式测速单个结果（Channel 推送）：delay=0 表示测速失败/超时。
#[derive(Serialize)]
pub struct DelayResult {
    pub name: String,
    pub delay: u32,
}

/// 流式批量测速：并发对全组每个节点调 `/proxies/{name}/delay`，测完一个即经 Channel 推送，
/// 前端增量更新 delayMap。替代 mihomo 批量端点 `/group/{name}/delay`——后者需等全组（含 5s
/// 超时的死节点）结算才一次返回，好节点被拖累致整体等 5-8s；流式下好节点百毫秒级即显示。
/// 并发度与 mihomo 批量端点内部一致（全并发），localhost 单节点 HTTP 开销可忽略。
#[tauri::command]
pub async fn proxy_test_group_delay_stream(
    state: State<'_, ProxyState>,
    group: String,
    on_event: Channel<DelayResult>,
) -> Result<(), String> {
    let (base, secret) = controller_endpoint(&state)?;
    // 取全组节点列表（一次 GET /proxies，响应结构 { proxies: { [group]: { all: [...] } } }）
    let proxies = controller::get_proxies(&base, &secret).await?;
    let nodes: Vec<String> = proxies
        .get("proxies")
        .and_then(|p| p.get(group.as_str()))
        .and_then(|g| g.get("all"))
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    // 并发测速：每节点独立 spawn，测完即推送，死节点 5s 超时不阻塞好节点。
    // base/secret 是 String 需 Arc 共享不可克隆所有权；Channel 本身 impl Clone（内部已含 Arc）。
    let base = Arc::new(base);
    let secret = Arc::new(secret);
    let mut set = JoinSet::new();
    for name in nodes {
        let (base, secret, on_event) = (base.clone(), secret.clone(), on_event.clone());
        set.spawn(async move {
            let delay = controller::test_delay(&base, &secret, &name)
                .await
                .unwrap_or(0);
            let _ = on_event.send(DelayResult { name, delay });
        });
    }
    while set.join_next().await.is_some() {}
    Ok(())
}

/// PATCH /configs → 切换规则模式。
#[tauri::command]
pub async fn proxy_set_mode(
    app: AppHandle,
    state: State<'_, ProxyState>,
    mode: String,
) -> Result<(), String> {
    let cur = state
        .run_params
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|p| p.mode.clone()))
        .unwrap_or_default();
    if cur == mode {
        return Ok(());
    }
    let (base, secret) = controller_endpoint(&state)?;
    controller::set_mode(&base, &secret, &mode).await?;
    if let Ok(mut guard) = state.run_params.lock() {
        if let Some(p) = guard.as_mut() {
            p.mode = mode.clone();
        }
    }
    let _ = app.emit("proxy-mode", mode);
    crate::runtime::menubar::refresh(&app);
    Ok(())
}

/// 免提权软重启（热重载 active config）。
#[tauri::command]
pub async fn proxy_reconnect(app: AppHandle, state: State<'_, ProxyState>) -> Result<(), String> {
    let mut params = state
        .run_params
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "代理未开启".to_string())?;
    let base = format!("http://127.0.0.1:{}", params.controller_port);
    if !root_mihomo_running(&app) {
        return Err("代理核心已退出，请关闭后重新开启".into());
    }
    if !controller::check_auth(&base, &params.secret)
        .await
        .unwrap_or(false)
    {
        return Err("代理核心无响应，请关闭后重新开启".into());
    }
    params.tun = true;
    let log_before = lifecycle::log_size(&app); // reload 前快照，供 verify_tun_active 区分新增行
    reload_config_yaml(&app, &params).await?;
    // 同步 TUN 验证：失败时回滚 idle config 清理 mihomo 状态（同 start_core）
    if let Err(e) = lifecycle::verify_tun_active(&app, log_before).await {
        lifecycle::rollback_to_idle(&app, &params).await;
        return Err(e);
    }
    state.enabled.store(true, Ordering::Relaxed);
    state.tun_active.store(true, Ordering::Relaxed);
    ensure_monitor(&app);
    let _ = app.emit("proxy-enabled", true);
    Ok(())
}

/// GET /rules → 分流规则列表。
#[tauri::command]
pub async fn proxy_get_rules(state: State<'_, ProxyState>) -> Result<Value, String> {
    let Some((port, secret)) = controller_creds_opt(&state) else {
        return Ok(serde_json::json!({ "rules": [] }));
    };
    let base = format!("http://127.0.0.1:{port}");
    Ok(controller::get_rules(&base, &secret)
        .await
        .unwrap_or_else(|_| serde_json::json!({ "rules": [] })))
}

/// 开 /traffic WS 流。
#[tauri::command]
pub async fn proxy_traffic_stream(
    app: AppHandle,
    state: State<'_, ProxyState>,
    on_event: Channel<TrafficFrame>,
) -> Result<(), String> {
    let Some((port, secret)) = controller_creds_opt(&state) else {
        return Ok(());
    };
    let token = app
        .state::<StreamRegistry>()
        .register(stream::ID_TRAFFIC.into());
    tauri::async_runtime::spawn(async move {
        stream::traffic_loop(port, &secret, token, on_event).await;
    });
    Ok(())
}

/// 开 /connections WS 流。
#[tauri::command]
pub async fn proxy_connections_stream(
    app: AppHandle,
    state: State<'_, ProxyState>,
    on_event: Channel<Value>,
) -> Result<(), String> {
    let Some((port, secret)) = controller_creds_opt(&state) else {
        return Ok(());
    };
    let token = app
        .state::<StreamRegistry>()
        .register(stream::ID_CONNECTIONS.into());
    tauri::async_runtime::spawn(async move {
        stream::connections_loop(port, &secret, token, on_event).await;
    });
    Ok(())
}

/// 开 /logs WS 流。
#[tauri::command]
pub async fn proxy_logs_stream(
    app: AppHandle,
    state: State<'_, ProxyState>,
    level: String,
    on_event: Channel<LogFrame>,
) -> Result<(), String> {
    let Some((port, secret)) = controller_creds_opt(&state) else {
        return Ok(());
    };
    let token = app
        .state::<StreamRegistry>()
        .register(stream::ID_LOGS.into());
    tauri::async_runtime::spawn(async move {
        stream::logs_loop(port, &secret, &level, token, on_event).await;
    });
    Ok(())
}

/// 停止指定 WS 流。
#[tauri::command]
pub async fn proxy_stop_stream(state: State<'_, StreamRegistry>, id: String) -> Result<(), String> {
    state.cancel(&id);
    Ok(())
}

/// Proxy 扩展。
pub struct ProxyExtension;

#[async_trait::async_trait]
impl Extension for ProxyExtension {
    fn id(&self) -> &'static str {
        "proxy"
    }

    async fn setup(&self, app: &AppHandle) -> tauri::Result<()> {
        app.manage(ProxyState {
            enabled: AtomicBool::new(false),
            run_params: Mutex::new(None),
            tun_active: AtomicBool::new(false),
            current_node: Mutex::new(None),
            monitor_gen: AtomicU64::new(0),
            monitor_spawned_gen: AtomicU64::new(u64::MAX), // 初始无 task：任意代都不等于「已 spawn」
            release_gen: AtomicU64::new(0),
        });
        app.manage(StreamRegistry::default());
        menu::register();
        lifecycle::observe_tun_taken(app);
        let app2 = app.clone();
        tauri::async_runtime::spawn(async move {
            lifecycle::reconnect_root_mihomo(&app2).await;
            let _ = core::ensure_geo_files(&app2).await;
        });
        Ok(())
    }
}
