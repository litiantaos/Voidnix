mod controller;
mod core;
mod subscription;
mod system_proxy;
mod tun;

use crate::runtime::registry::Extension;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, State};

use self::core::{ManagedChild, ProxyCore, RunParams};

/// 代理运行状态：enabled 标志 + mihomo 子进程托管 + 最近一次启动参数（供热重启）
/// + system_proxy_active 标记（仅当本扩展设置过系统代理时才在关闭时清除，避免误清用户其它代理）
/// + tun_active 标记（mihomo 是否以 root 运行；一旦 root 持续 true，TUN 开关热切换不重启不提权）。
pub struct ProxyState {
    pub enabled: AtomicBool,
    pub core: ProxyCore,
    pub run_params: Mutex<Option<RunParams>>,
    pub system_proxy_active: AtomicBool,
    pub tun_active: AtomicBool,
}

/// 启用/停用代理核心（启动或终止 mihomo）。
///
/// tun=true 时 mihomo 以 root 运行（osascript 提权，无 Child 句柄，tun_active 标记）；
/// tun=false 时以当前用户运行（ManagedChild 托管）。
/// run 参数（端口/secret/mode/tun）由前端 config 传入，Rust 仅消费不回读 plugin-store。
#[tauri::command]
#[allow(clippy::too_many_arguments)] // Tauri 命令 IPC 契约决定参数数（state 注入 + 前端参数）
pub async fn set_proxy_enabled(
    app: AppHandle,
    state: State<'_, ProxyState>,
    enabled: bool,
    mixed_port: u16,
    controller_port: u16,
    secret: String,
    mode: String,
    tun: bool,
) -> Result<bool, String> {
    if enabled {
        // 已在运行（含 TUN）视为成功。锁用块作用域包裹，确保 MutexGuard 不跨 await（Send 分析）。
        {
            let guard = state.core.process.lock().map_err(|e| e.to_string())?;
            if guard.is_some() || state.tun_active.load(Ordering::Relaxed) {
                return Ok(true);
            }
        }
        let params = RunParams {
            mixed_port,
            controller_port,
            secret,
            mode,
            tun,
        };
        let base = format!("http://127.0.0.1:{controller_port}");
        if tun {
            tun::spawn_root(&app, &params).await?;
            // 等 controller 就绪；失败回滚（停 root 实例）
            if let Err(e) = controller::wait_ready(&base, &params.secret, 8000).await {
                let _ = tun::stop_root(&app);
                return Err(e);
            }
            state.tun_active.store(true, Ordering::Relaxed);
        } else {
            let child = core::spawn(&app, &params).await?;
            {
                let mut g = state.core.process.lock().map_err(|e| e.to_string())?;
                *g = Some(child);
            }
            // 等 controller 就绪；失败回滚（停 child）
            if let Err(e) = controller::wait_ready(&base, &params.secret, 8000).await {
                let mut g = state.core.process.lock().map_err(|e| e.to_string())?;
                if let Some(mut managed) = g.take() {
                    managed.shutdown();
                }
                return Err(e);
            }
        }
        *state.run_params.lock().map_err(|e| e.to_string())? = Some(params);
        state.enabled.store(true, Ordering::Relaxed);
        Ok(true)
    } else {
        // 若本扩展设置过系统代理，关闭前清除（恢复直连），避免指向已停核心导致断网。
        if state.system_proxy_active.swap(false, Ordering::Relaxed) {
            let _ = system_proxy::apply(0, false);
        }
        if state.tun_active.swap(false, Ordering::Relaxed) {
            tun::stop_root(&app)?;
        } else {
            let child_opt = state.core.process.lock().map_err(|e| e.to_string())?.take();
            if let Some(mut managed) = child_opt {
                managed.shutdown();
            }
        }
        state.enabled.store(false, Ordering::Relaxed);
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

/// 拉取订阅并持久化（subs/<id>.yaml），返回节点数；核心运行中则热重启以应用新配置。
#[tauri::command]
pub async fn proxy_update_subscription(
    app: AppHandle,
    state: State<'_, ProxyState>,
    id: String,
    url: String,
) -> Result<usize, String> {
    let (count, text) = subscription::fetch(&url).await?;
    subscription::save(&app, &id, &text)?;
    restart_if_running(&app, &state).await?;
    Ok(count)
}

/// 删除订阅持久化文件；核心运行中则热重启以移除其节点。
#[tauri::command]
pub async fn proxy_remove_subscription(
    app: AppHandle,
    state: State<'_, ProxyState>,
    id: String,
) -> Result<(), String> {
    subscription::remove(&app, &id)?;
    restart_if_running(&app, &state).await?;
    Ok(())
}

/// 核心运行中时重启以应用配置变更（订阅增删改）。停用状态直接跳过。
/// 重启后等 controller 就绪再返回，避免前端立即查询撞上未 bind 的端口。
async fn restart_if_running(app: &AppHandle, state: &ProxyState) -> Result<(), String> {
    if !state.enabled.load(Ordering::Relaxed) {
        return Ok(());
    }
    let params = state
        .run_params
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "core running but no run params".to_string())?;

    // 终止旧进程（块作用域锁，避免 MutexGuard 跨 await）
    let child_opt = state.core.process.lock().map_err(|e| e.to_string())?.take();
    if let Some(mut managed) = child_opt {
        managed.shutdown();
    }

    // 重新生成 config.yaml 并拉起新进程
    let new_child: ManagedChild = core::spawn(app, &params).await?;
    {
        let mut guard = state.core.process.lock().map_err(|e| e.to_string())?;
        *guard = Some(new_child);
    }

    // 等 controller 就绪（订阅热重启后新进程需重新 bind）
    let base = format!("http://127.0.0.1:{}", params.controller_port);
    controller::wait_ready(&base, &params.secret, 8000).await
}

/// 取 mihomo controller endpoint（base URL + secret），核心未运行时报错。
fn controller_endpoint(state: &ProxyState) -> Result<(String, String), String> {
    let params = state
        .run_params
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "代理核心未运行".to_string())?;
    Ok((
        format!("http://127.0.0.1:{}", params.controller_port),
        params.secret,
    ))
}

/// GET /proxies → 完整代理树。
#[tauri::command]
pub async fn proxy_get_proxies(state: State<'_, ProxyState>) -> Result<Value, String> {
    let (base, secret) = controller_endpoint(&state)?;
    controller::get_proxies(&base, &secret).await
}

/// PUT /proxies/{group} → 在 selector 分组选择节点。
#[tauri::command]
pub async fn proxy_select_proxy(
    state: State<'_, ProxyState>,
    group: String,
    name: String,
) -> Result<(), String> {
    let (base, secret) = controller_endpoint(&state)?;
    controller::select_proxy(&base, &secret, &group, &name).await
}

/// GET /proxies/{name}/delay → 延迟测速（ms，失败为 0）。
#[tauri::command]
pub async fn proxy_test_delay(state: State<'_, ProxyState>, name: String) -> Result<u32, String> {
    let (base, secret) = controller_endpoint(&state)?;
    controller::test_delay(&base, &secret, &name).await
}

/// PATCH /configs → 切换规则模式。
#[tauri::command]
pub async fn proxy_set_mode(state: State<'_, ProxyState>, mode: String) -> Result<(), String> {
    let (base, secret) = controller_endpoint(&state)?;
    controller::set_mode(&base, &secret, &mode).await
}

/// 设/清 macOS 系统代理（HTTP/HTTPS/SOCKS 指向 127.0.0.1:mixedPort）。
/// 成功后更新 system_proxy_active 标记（关闭核心时据此清除）。
#[tauri::command]
pub async fn proxy_set_system_proxy(
    state: State<'_, ProxyState>,
    enabled: bool,
) -> Result<bool, String> {
    let port = state
        .run_params
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .map(|p| p.mixed_port)
        .ok_or_else(|| "代理核心未运行".to_string())?;
    system_proxy::apply(port, enabled)?;
    state.system_proxy_active.store(enabled, Ordering::Relaxed);
    Ok(enabled)
}

/// 切换 TUN 模式。
///
/// mihomo 已 root 运行时（tun_active）：热重载 config（改 tun 段 + PUT /configs），
/// 不重启进程、不提权——TUN 开关切换仅首次提权一次。
/// user 模式时：停 user mihomo + 起 root mihomo（osascript 提权一次）。
#[tauri::command]
pub async fn proxy_enable_tun(
    app: AppHandle,
    state: State<'_, ProxyState>,
    tun: bool,
) -> Result<bool, String> {
    let mut params = state
        .run_params
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "代理核心未运行".to_string())?;
    if params.tun == tun {
        return Ok(tun);
    }
    params.tun = tun;
    let base = format!("http://127.0.0.1:{}", params.controller_port);
    let secret = params.secret.clone();

    if state.tun_active.load(Ordering::Relaxed) {
        // mihomo 已 root 运行：热切换 tun（仅重写 config.yaml + PUT /configs reload），免重启免提权
        let yaml = subscription::build_run_config(&app, &params)?;
        let path = core::run_config_path(&app)?;
        std::fs::write(&path, yaml).map_err(|e| e.to_string())?;
        controller::reload_config(&base, &secret, &path.to_string_lossy()).await?;
    } else {
        // user 模式 → 切 root（提权一次）：停 user mihomo + 起 root mihomo
        let managed_opt = state.core.process.lock().map_err(|e| e.to_string())?.take();
        if let Some(mut managed) = managed_opt {
            managed.shutdown();
        }
        tun::spawn_root(&app, &params).await?;
        controller::wait_ready(&base, &secret, 8000).await?;
        state.tun_active.store(true, Ordering::Relaxed);
    }
    *state.run_params.lock().map_err(|e| e.to_string())? = Some(params);
    Ok(tun)
}

/// 命令注册（局部 invoke_handler，§2.8）。
pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new("proxy").build()
}

/// Proxy 扩展。
pub struct ProxyExtension;

#[async_trait::async_trait]
impl Extension for ProxyExtension {
    fn id(&self) -> &'static str {
        "proxy"
    }

    async fn setup(&self, app: &AppHandle) -> tauri::Result<()> {
        use tauri::Manager;
        app.manage(ProxyState {
            enabled: AtomicBool::new(false),
            core: ProxyCore::new(),
            run_params: Mutex::new(None),
            system_proxy_active: AtomicBool::new(false),
            tun_active: AtomicBool::new(false),
        });
        Ok(())
    }
}
