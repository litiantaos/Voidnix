mod controller;
mod core;
mod subscription;
mod system_proxy;
mod tun;

use crate::runtime::menubar::{MenuBarContribution, MenuEntry};
use crate::runtime::registry::Extension;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

use self::core::{ProxyCore, RunParams};

/// 节点子菜单项 id 前缀（点击时按前缀剥离出节点名）。
const NODE_ITEM_PREFIX: &str = "proxy_node:";

/// 代理运行状态：enabled 标志 + mihomo 子进程托管 + 最近一次启动参数（供热重启）
/// + system_proxy_active 标记（仅当本扩展设置过系统代理时才在关闭时清除，避免误清用户其它代理）
/// + tun_active 标记（mihomo 是否以 root 运行；一旦 root 持续 true，TUN 开关热切换不重启不提权）。
pub struct ProxyState {
    pub enabled: AtomicBool,
    pub core: ProxyCore,
    pub run_params: Mutex<Option<RunParams>>,
    pub system_proxy_active: AtomicBool,
    pub tun_active: AtomicBool,
    /// 菜单栏节点子菜单缓存（name, 是否当前选中）；由 refresh_proxy_menu 异步拉取填充。
    pub menu_nodes: Mutex<Vec<(String, bool)>>,
    /// 菜单栏订阅子菜单缓存（订阅名列表）；由前端 proxy_sync_menu_subs 推送。
    pub menu_subs: Mutex<Vec<String>>,
}

/// 启用/停用代理核心（启动或终止 mihomo）。
///
/// tun=true 时 mihomo 以 root 运行（osascript 提权，无 Child 句柄，tun_active 标记）；
/// tun=false 时以当前用户运行（ManagedChild 托管），并自动设置 macOS 系统代理指向 mixed-port
///（TUN 模式由虚拟网卡接管全部流量，无需系统代理）。
/// run 参数（端口/secret/mode/tun）由前端 config 传入，Rust 仅消费不回读 plugin-store。
/// 启用成功后显示菜单栏托盘（开关代理/节点切换快捷入口），停用时隐藏。
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
        let params = RunParams {
            mixed_port,
            controller_port,
            secret,
            mode,
            tun,
        };
        start_core(&app, &state, params).await?;
        // user 模式自动设系统代理（TUN 模式由虚拟网卡接管全部流量，无需系统代理）。
        // best-effort：失败仅记日志不阻塞启用——用户仍可手动 curl -x 走 mixed-port。
        if !tun {
            match system_proxy::apply(mixed_port, true) {
                Ok(()) => state.system_proxy_active.store(true, Ordering::Relaxed),
                Err(e) => eprintln!("[proxy] enable 时设系统代理失败: {e}"),
            }
        }
        // 立即显示菜单栏图标（静态项），异步补节点子菜单
        crate::runtime::menubar::refresh(&app);
        let _ = app.emit("proxy-enabled", true);
        let app2 = app.clone();
        tauri::async_runtime::spawn(async move {
            refresh_proxy_menu(&app2).await;
        });
        Ok(true)
    } else {
        stop_core(&app, &state)?;
        crate::runtime::menubar::refresh(&app);
        let _ = app.emit("proxy-enabled", false);
        Ok(false)
    }
}

/// 启动 mihomo 核心（user 或 root 模式）。已在运行视为成功（幂等，支持重复调用）。
/// 锁用块作用域包裹，确保 MutexGuard 不跨 await（Send 分析）。
async fn start_core(app: &AppHandle, state: &ProxyState, params: RunParams) -> Result<(), String> {
    {
        let guard = state.core.process.lock().map_err(|e| e.to_string())?;
        if guard.is_some() || state.tun_active.load(Ordering::Relaxed) {
            return Ok(());
        }
    }
    let base = format!("http://127.0.0.1:{}", params.controller_port);
    if params.tun {
        tun::spawn_root(app, &params).await?;
        if let Err(e) = controller::wait_ready(&base, &params.secret, 8000).await {
            let _ = tun::stop_root(app); // 失败回滚（停 root 实例）
            return Err(e);
        }
        state.tun_active.store(true, Ordering::Relaxed);
    } else {
        let child = core::spawn(app, &params).await?;
        {
            let mut g = state.core.process.lock().map_err(|e| e.to_string())?;
            *g = Some(child);
        }
        if let Err(e) = controller::wait_ready(&base, &params.secret, 8000).await {
            let mut g = state.core.process.lock().map_err(|e| e.to_string())?;
            if let Some(mut managed) = g.take() {
                managed.shutdown(); // 失败回滚（停 child）
            }
            return Err(e);
        }
    }
    *state.run_params.lock().map_err(|e| e.to_string())? = Some(params);
    state.enabled.store(true, Ordering::Relaxed);
    Ok(())
}

/// 停止 mihomo 核心（同步：system_proxy/tun/child 三路停均为同步调用）。
/// load 判断 → 副作用成功后再 store(false)：取消授权/失败时状态不变，用户可重试，
/// 避免「swap 先于副作用」导致 tun_active 假清而 root 进程仍驻留。
fn stop_core(app: &AppHandle, state: &ProxyState) -> Result<(), String> {
    // 若本扩展设置过系统代理，关闭前清除（恢复直连），避免指向已停核心导致断网。
    // best-effort：apply 成功才清标记，失败保留 true 以便下次关闭重试。
    if state.system_proxy_active.load(Ordering::Relaxed) && system_proxy::apply(0, false).is_ok() {
        state.system_proxy_active.store(false, Ordering::Relaxed);
    }
    if state.tun_active.load(Ordering::Relaxed) {
        tun::stop_root(app)?;
        state.tun_active.store(false, Ordering::Relaxed);
    } else {
        let child_opt = state.core.process.lock().map_err(|e| e.to_string())?.take();
        if let Some(mut managed) = child_opt {
            managed.shutdown();
        }
    }
    state.enabled.store(false, Ordering::Relaxed);
    Ok(())
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
    reload_if_running(&app, &state).await?;
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
    reload_if_running(&app, &state).await?;
    Ok(())
}

/// 核心运行中时热重载以应用配置变更（订阅增删）。
///
/// 改用 controller `PUT /configs {path}` 热重载而非进程重启：重建 config.yaml 后通知
/// mihomo 重新加载即可，user/root 两种模式统一生效——避免 TUN 模式下进程重启会漏停
/// root 实例 + 拉起无权限的 user 子进程（端口冲突）的死局。停用状态直接跳过。
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

    // 重建 config.yaml（合并 subs/*.yaml 最新订阅）
    let yaml = subscription::build_run_config(app, &params)?;
    let path = core::run_config_path(app)?;
    std::fs::write(&path, yaml).map_err(|e| e.to_string())?;

    // PUT /configs {path} → mihomo 原生热重载（proxies/groups/rules 全量刷新）
    let base = format!("http://127.0.0.1:{}", params.controller_port);
    controller::reload_config(&base, &params.secret, &path.to_string_lossy()).await?;

    // 节点列表可能变化，刷新聚合菜单子菜单（best-effort，不阻塞命令返回）
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        refresh_proxy_menu(&app2).await;
    });
    Ok(())
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

/// PUT /proxies/{group} → 在 selector 分组选择节点。成功后刷新托盘子菜单选中标记。
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
        refresh_proxy_menu(&app2).await;
    });
    Ok(())
}

/// GET /proxies/{name}/delay → 延迟测速（ms，失败为 0）。
#[tauri::command]
pub async fn proxy_test_delay(state: State<'_, ProxyState>, name: String) -> Result<u32, String> {
    let (base, secret) = controller_endpoint(&state)?;
    controller::test_delay(&base, &secret, &name).await
}

/// 前端推送订阅名列表 → 缓存 + 重建菜单（订阅子菜单展示订阅名，点击打开面板）。
#[tauri::command]
pub async fn proxy_sync_menu_subs(
    app: AppHandle,
    state: State<'_, ProxyState>,
    names: Vec<String>,
) -> Result<(), String> {
    *crate::runtime::lock_or_recover(&state.menu_subs) = names;
    crate::runtime::menubar::refresh(&app);
    Ok(())
}

/// PATCH /configs → 切换规则模式。成功后回写 run_params.mode，确保后续 reload_if_running
/// 重建 config.yaml 时不会回退到启用时缓存的旧模式。
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
        return Ok(()); // 未变，跳过（防前端回声）
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
    apply_tun(&app, &state, tun).await
}

/// TUN 切换核心逻辑（命令与托盘菜单共用）。
async fn apply_tun(app: &AppHandle, state: &ProxyState, tun: bool) -> Result<bool, String> {
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
        let yaml = subscription::build_run_config(app, &params)?;
        let path = core::run_config_path(app)?;
        std::fs::write(&path, yaml).map_err(|e| e.to_string())?;
        controller::reload_config(&base, &secret, &path.to_string_lossy()).await?;
    } else {
        // user 模式 → 切 root（提权一次）：停 user mihomo + 起 root mihomo
        let managed_opt = state.core.process.lock().map_err(|e| e.to_string())?.take();
        if let Some(mut managed) = managed_opt {
            managed.shutdown();
        }
        tun::spawn_root(app, &params).await?;
        controller::wait_ready(&base, &secret, 8000).await?;
        state.tun_active.store(true, Ordering::Relaxed);
    }
    // 切换后同步系统代理：切到 TUN 时清除（虚拟网卡接管，系统代理冗余），
    // 切回 user 时重设（让系统流量经 mixed-port 走 mihomo）。best-effort 不阻断切换。
    if tun {
        if state.system_proxy_active.load(Ordering::Relaxed)
            && system_proxy::apply(0, false).is_ok()
        {
            state.system_proxy_active.store(false, Ordering::Relaxed);
        }
    } else if system_proxy::apply(params.mixed_port, true).is_ok() {
        state.system_proxy_active.store(true, Ordering::Relaxed);
    }
    *state.run_params.lock().map_err(|e| e.to_string())? = Some(params);
    // 广播新 TUN 状态：托盘切换时前端 config.tunMode 会与之失步（Rust 不回写 plugin-store），
    // 由前端监听同步。命令路径（前端已先设 config）收到的是同值，无副作用。
    let _ = app.emit("proxy-tun", tun);
    crate::runtime::menubar::refresh(app);
    Ok(tun)
}

// ── 聚合菜单栏贡献（代理开启后向框架统一托盘贡献：开关/TUN/面板/节点切换） ──

/// 拉取最新节点缓存 + 重建聚合菜单（best-effort，controller 不可达时保留上次缓存）。
async fn refresh_proxy_menu(app: &AppHandle) {
    let state = app.state::<ProxyState>();
    if let Ok((base, secret)) = controller_endpoint(&state) {
        if let Ok(val) = controller::get_proxies(&base, &secret).await {
            let (_, nodes) = parse_main_group(&val);
            *crate::runtime::lock_or_recover(&state.menu_nodes) = nodes;
        }
    }
    crate::runtime::menubar::refresh(app);
}

/// 菜单快照：镜像界面「代理」分组（开启代理/TUN 勾选 + 规则模式二级菜单）+ 订阅 + 节点子菜单。
/// 文案与界面 View.vue 一致；未激活返回空。
fn build_proxy(app: &AppHandle) -> Vec<MenuEntry> {
    let state = app.state::<ProxyState>();
    if !state.enabled.load(Ordering::Relaxed) {
        return vec![];
    }
    let tun_on = state.tun_active.load(Ordering::Relaxed);
    let mode = crate::runtime::lock_or_recover(&state.run_params)
        .as_ref()
        .map(|p| p.mode.clone())
        .unwrap_or_else(|| "rule".to_string());
    let mode_label = match mode.as_str() {
        "global" => "全局",
        "direct" => "直连",
        _ => "规则",
    };
    let mut entries = vec![
        MenuEntry::CheckItem {
            id: "proxy_toggle".into(),
            label: "开启代理".into(),
            checked: true,
        },
        MenuEntry::CheckItem {
            id: "proxy_tun".into(),
            label: "TUN 模式".into(),
            checked: tun_on,
        },
        MenuEntry::Submenu {
            label: format!("规则模式：{mode_label}"),
            items: vec![
                MenuEntry::CheckItem {
                    id: "proxy_mode_rule".into(),
                    label: "规则".into(),
                    checked: mode == "rule",
                },
                MenuEntry::CheckItem {
                    id: "proxy_mode_global".into(),
                    label: "全局".into(),
                    checked: mode == "global",
                },
                MenuEntry::CheckItem {
                    id: "proxy_mode_direct".into(),
                    label: "直连".into(),
                    checked: mode == "direct",
                },
            ],
        },
    ];
    // 订阅子菜单（订阅名由前端 proxy_sync_menu_subs 推送；点击打开面板）
    let subs = crate::runtime::lock_or_recover(&state.menu_subs).clone();
    if !subs.is_empty() {
        // 单订阅且名非空 → 显示订阅名；否则显示数量
        let label = match subs.as_slice() {
            [only] if !only.is_empty() => format!("订阅：{only}"),
            _ => format!("订阅（{}）", subs.len()),
        };
        entries.push(MenuEntry::Submenu {
            label,
            items: subs
                .iter()
                .map(|s| MenuEntry::Item {
                    id: format!("proxy_sub:{s}"),
                    label: s.clone(),
                    enabled: true,
                })
                .collect(),
        });
    }
    // 节点子菜单（缓存由 refresh_proxy_menu 异步拉取填充；父项带当前选中节点名）
    let nodes = crate::runtime::lock_or_recover(&state.menu_nodes).clone();
    if !nodes.is_empty() {
        let current = nodes
            .iter()
            .find(|(_, checked)| *checked)
            .map(|(n, _)| n.as_str());
        let label = match current {
            Some(n) => format!("节点：{n}"),
            None => "节点".to_string(),
        };
        entries.push(MenuEntry::Submenu {
            label,
            items: nodes
                .iter()
                .map(|(name, checked)| MenuEntry::CheckItem {
                    id: format!("{NODE_ITEM_PREFIX}{name}"),
                    label: name.clone(),
                    checked: *checked,
                })
                .collect(),
        });
    }
    entries
}

/// 菜单点击分派（均复用命令，内部 emit 同步前端 + refresh）。
fn on_proxy_event(app: &AppHandle, id: &str) {
    match id {
        // 开启代理 CheckItem（菜单仅 enabled 时显示，点击 = 关闭）
        "proxy_toggle" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<ProxyState>();
                if let Err(e) = stop_core(&app, &state) {
                    eprintln!("[proxy] 菜单关闭代理: {e}");
                }
                let _ = app.emit("proxy-enabled", false);
                crate::runtime::menubar::refresh(&app);
            });
        }
        "proxy_tun" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<ProxyState>();
                let cur = state.tun_active.load(Ordering::Relaxed);
                let _ = apply_tun(&app, &state, !cur).await; // apply_tun 内部 emit proxy-tun + refresh
            });
        }
        "proxy_mode_rule" | "proxy_mode_global" | "proxy_mode_direct" => {
            let mode = if id == "proxy_mode_rule" {
                "rule"
            } else if id == "proxy_mode_global" {
                "global"
            } else {
                "direct"
            }
            .to_string();
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<ProxyState>();
                let _ = proxy_set_mode(app.clone(), state, mode).await;
            });
        }
        other if other.starts_with("proxy_sub:") => {
            // 订阅项点击 → 打开代理面板（show_main 触 NSWindow 须主线程）
            let app2 = app.clone();
            let _ = app.run_on_main_thread(move || {
                crate::runtime::window::show_main(&app2);
                let _ = app2.emit("open-module", "proxy");
            });
        }
        other if other.starts_with(NODE_ITEM_PREFIX) => {
            let name = other[NODE_ITEM_PREFIX.len()..].to_string();
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<ProxyState>();
                let Ok((base, secret)) = controller_endpoint(&state) else {
                    return;
                };
                // 取当前主分组名（订阅变更后分组名可能变，实时查不缓存）
                let proxies = controller::get_proxies(&base, &secret)
                    .await
                    .unwrap_or(Value::Null);
                if let Some(group) = parse_main_group(&proxies).0 {
                    let _ = controller::select_proxy(&base, &secret, &group, &name).await;
                }
                refresh_proxy_menu(&app).await; // 更新菜单选中标记
                let _ = app.emit("proxy-node", &name); // 通知面板刷新节点选中
            });
        }
        _ => {}
    }
}

/// 解析 /proxies 响应取主分组（首个非 GLOBAL 的 Selector，回退 GLOBAL）及其节点列表。
/// 返回 (分组名, [(节点名, 是否当前选中)])。镜像 logic.ts pickMainGroup 语义。
fn parse_main_group(val: &Value) -> (Option<String>, Vec<(String, bool)>) {
    let Some(proxies) = val.get("proxies").and_then(|p| p.as_object()) else {
        return (None, Vec::new());
    };
    let main = proxies
        .iter()
        .find(|(k, e)| {
            e.get("type").and_then(|t| t.as_str()) == Some("Selector") && k.as_str() != "GLOBAL"
        })
        .map(|(_, v)| v)
        .or_else(|| proxies.get("GLOBAL"));
    let Some(g) = main else {
        return (None, Vec::new());
    };
    let group_name = g.get("name").and_then(|n| n.as_str()).map(String::from);
    let now = g
        .get("now")
        .and_then(|n| n.as_str())
        .unwrap_or("")
        .to_string();
    let nodes = g
        .get("all")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|n| n.as_str().map(String::from))
                .map(|name| {
                    let selected = name == now;
                    (name, selected)
                })
                .collect()
        })
        .unwrap_or_default();
    (group_name, nodes)
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
        app.manage(ProxyState {
            enabled: AtomicBool::new(false),
            core: ProxyCore::new(),
            run_params: Mutex::new(None),
            system_proxy_active: AtomicBool::new(false),
            tun_active: AtomicBool::new(false),
            menu_nodes: Mutex::new(Vec::new()),
            menu_subs: Mutex::new(Vec::new()),
        });
        crate::runtime::menubar::register(MenuBarContribution {
            title: "代理",
            build: Arc::new(build_proxy),
            on_event: Arc::new(on_proxy_event),
        });
        Ok(())
    }
}
