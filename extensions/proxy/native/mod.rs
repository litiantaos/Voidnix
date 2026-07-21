//! 代理扩展：命令入口 + 插件/生命周期装配。
//! 核心状态机见 lifecycle；菜单栏见 menu；mihomo 进程/订阅/流见 core/tun/subscription/stream。

mod controller;
mod core;
mod lifecycle;
mod menu;
mod stream;
mod subscription;
mod tun;

use crate::runtime::registry::Extension;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager, State};

use self::core::RunParams;
use self::lifecycle::{
    controller_creds_opt, controller_endpoint, ensure_monitor, reload_config_yaml,
    reload_if_running, root_mihomo_running, start_core, stop_core, ProxyState,
};
use self::stream::{LogFrame, StreamRegistry, TrafficFrame};

/// 启用/停用代理（统一 TUN 模式：root mihomo 常驻 + 热重载 active/idle）。
#[tauri::command]
pub async fn set_proxy_enabled(
    app: AppHandle,
    state: State<'_, ProxyState>,
    enabled: bool,
    mixed_port: u16,
    controller_port: u16,
    secret: String,
    mode: String,
) -> Result<bool, String> {
    if enabled {
        let params = RunParams {
            mixed_port,
            controller_port,
            secret,
            mode,
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

/// 查询内核状态（已下载/版本号/下载中），供列表「内核」项展示。
#[tauri::command]
pub async fn proxy_core_status(app: AppHandle) -> Result<core::CoreStatus, String> {
    Ok(core::core_status(&app))
}

/// 强制触发内核下载（未下载时）；前端「内核」项下载按钮调用。
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

/// 更新内核：停代理（若在跑）→ kill root 进程 → 删旧 binary → ensure_bin 重下最新 → 恢复。
#[tauri::command]
pub async fn proxy_update_core(app: AppHandle, state: State<'_, ProxyState>) -> Result<(), String> {
    let was_enabled = state.enabled.load(Ordering::Relaxed);
    let params = state.run_params.lock().map_err(|e| e.to_string())?.clone();
    if state.tun_active.load(Ordering::Relaxed) {
        tun::stop_root(&app)?;
        state.tun_active.store(false, Ordering::Relaxed);
    }
    state.enabled.store(false, Ordering::Relaxed);
    core::remove_core_files(&app)?;
    core::ensure_bin(&app).await?;
    if was_enabled {
        if let Some(p) = params {
            start_core(&app, &state, p).await?;
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
    reload_if_running(&app, &state).await?;
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        menu::refresh_proxy_menu(&app2).await;
    });
    Ok(count)
}

/// 删除订阅持久化文件；核心运行中则热重载。
#[tauri::command]
pub async fn proxy_remove_subscription(
    app: AppHandle,
    state: State<'_, ProxyState>,
    id: String,
) -> Result<(), String> {
    subscription::remove(&app, &id)?;
    reload_if_running(&app, &state).await?;
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

/// GET /proxies/{name}/delay → 延迟测速（ms，失败为 0）。
#[tauri::command]
pub async fn proxy_test_delay(state: State<'_, ProxyState>, name: String) -> Result<u32, String> {
    let (base, secret) = controller_endpoint(&state)?;
    controller::test_delay(&base, &secret, &name).await
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
    reload_config_yaml(&app, &params).await?;
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
            monitor_alive: AtomicBool::new(false),
        });
        app.manage(StreamRegistry::default());
        menu::register();
        let app2 = app.clone();
        tauri::async_runtime::spawn(async move {
            lifecycle::reconnect_root_mihomo(&app2).await;
            let _ = core::ensure_geo_files(&app2).await;
        });
        Ok(())
    }
}
