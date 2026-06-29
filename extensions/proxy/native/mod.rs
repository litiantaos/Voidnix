mod controller;
mod core;
mod subscription;
mod tun;

use crate::runtime::menubar::{MenuBarContribution, MenuEntry};
use crate::runtime::registry::Extension;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

use self::core::RunParams;

/// 节点子菜单项 id 前缀（点击时按前缀剥离出节点名）。
const NODE_ITEM_PREFIX: &str = "proxy_node:";

/// 代理运行状态：enabled（流量是否被代理）+ tun_active（root mihomo 进程是否在跑，常驻）
/// + run_params（最近一次 active 参数，供热重载/复用）。
pub struct ProxyState {
    pub enabled: AtomicBool,
    pub run_params: Mutex<Option<RunParams>>,
    pub tun_active: AtomicBool,
    /// 菜单栏节点子菜单缓存（name, 是否当前选中）；由 refresh_proxy_menu 异步拉取填充。
    pub menu_nodes: Mutex<Vec<(String, bool)>>,
    /// 菜单栏订阅子菜单缓存（订阅名列表）；由前端 proxy_sync_menu_subs 推送。
    pub menu_subs: Mutex<Vec<String>>,
}

/// 启用/停用代理（统一 TUN 模式：root mihomo 常驻 + 热重载 active/idle）。
///
/// 启用：`ensure_root_mihomo`（首次提权 spawn，之后幂等）+ 热重载 active config；常驻后免再提权。
/// 停用：热重载 idle config（mode=direct + 无 tun 段）→ mihomo 撤销 utun、流量直通，进程保留。
/// run 参数（端口/secret/mode）由前端 config 传入，Rust 仅消费不回读 plugin-store。
/// 启用后显示菜单栏托盘（开关代理/节点切换快捷入口），停用（idle 常驻）保留精简菜单。
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
        // 立即显示菜单栏图标（静态项），异步补节点子菜单
        crate::runtime::menubar::refresh(&app);
        let _ = app.emit("proxy-enabled", true);
        let app2 = app.clone();
        tauri::async_runtime::spawn(async move {
            refresh_proxy_menu(&app2).await;
        });
        Ok(true)
    } else {
        stop_core(&app, &state).await?;
        crate::runtime::menubar::refresh(&app);
        let _ = app.emit("proxy-enabled", false);
        Ok(false)
    }
}

/// 写 config.yaml + PUT /configs 热重载（active/idle 切换、订阅变更共用）。
/// TUN 模式下 root mihomo 常驻，代理开关 = 热重载 active/idle config，免 spawn 免提权。
async fn reload_config_yaml(app: &AppHandle, params: &RunParams) -> Result<(), String> {
    let yaml = subscription::build_run_config(app, params)?;
    let path = core::run_config_path(app)?;
    std::fs::write(&path, yaml).map_err(|e| e.to_string())?;
    let base = format!("http://127.0.0.1:{}", params.controller_port);
    controller::reload_config(&base, &params.secret, &path.to_string_lossy()).await
}

/// spawn root mihomo + wait_ready（失败回滚 stop_root 防端口占用残留）。
async fn spawn_and_wait(app: &AppHandle, params: &RunParams) -> Result<(), String> {
    let base = format!("http://127.0.0.1:{}", params.controller_port);
    tun::spawn_root(app, params).await?;
    if let Err(e) = controller::wait_ready(&base, &params.secret, 8000).await {
        let _ = tun::stop_root(app);
        return Err(e);
    }
    Ok(())
}

/// restart_root + wait_ready。失败不清理（restart_root 含杀旧逻辑，下次重启自动回收）。
async fn restart_and_wait(app: &AppHandle, params: &RunParams) -> Result<(), String> {
    let base = format!("http://127.0.0.1:{}", params.controller_port);
    tun::restart_root(app, params).await?;
    controller::wait_ready(&base, &params.secret, 8000).await
}

/// 确保 root mihomo 进程在跑（常驻）。已常驻则幂等返回；否则 spawn_root（提权一次）
/// 并 wait_ready（失败回滚 stop_root）。一旦启动，进程常驻到显式 stop_root 或 app
/// 退出——代理开关与 TUN 切换均走热重载，免再提权。
///
/// 返回 `true` 表示本次新 spawn（提权一次），`false` 表示复用已常驻进程。
///
/// 仅信任 `tun_active=true` 的进程（reconnect 成功置 idle 或本 session spawn 的），
/// 可安全热重载复用。`tun_active=false` 但有遗留进程（reconnect 未完成或失败）→ 状态
/// 不可信（secret 可能不匹配、TUN 可能已激活），restart_root 单次提权杀旧+启新。
async fn ensure_root_mihomo(
    app: &AppHandle,
    state: &ProxyState,
    params: &RunParams,
) -> Result<bool, String> {
    if state.tun_active.load(Ordering::Relaxed) {
        return Ok(false); // 本 session 已知状态，可热重载复用
    }
    if root_mihomo_running(app) {
        restart_and_wait(app, params).await?;
    } else {
        spawn_and_wait(app, params).await?;
    }
    state.tun_active.store(true, Ordering::Relaxed);
    Ok(true)
}

/// 启动代理（统一 TUN 模式）。已开启视为成功（幂等）。
///
/// root mihomo 常驻：`ensure_root_mihomo` 确保 spawn（首次/遗留清理）或复用已知状态进程。
/// 复用路径热重载 active config 恢复代理；热重载失败（罕见边缘情况）则 restart_and_wait
/// 单次提权重启。
async fn start_core(app: &AppHandle, state: &ProxyState, params: RunParams) -> Result<(), String> {
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
    Ok(())
}

/// 停止代理（流量切直通）。
///
/// root mihomo 常驻：**不 kill 进程**，热重载 idle config（mode=direct + 无 tun 段）→ mihomo
/// 撤销 utun、流量直通（被墙不可达，符合「关闭」语义），进程保留以便下次免提权恢复。
/// 热重载失败必须上抛：否则 enabled=false 但 mihomo 仍跑 active config（含 tun 段），流量
/// 持续被代理、utun 未撤——与 stop_root「验证确死否则报错」的关闭可靠性保持一致语义。
async fn stop_core(app: &AppHandle, state: &ProxyState) -> Result<(), String> {
    if state.tun_active.load(Ordering::Relaxed) {
        let idle = state.run_params.lock().map_err(|e| e.to_string())?.clone();
        if let Some(mut p) = idle {
            p.mode = "direct".into();
            p.tun = false;
            // 失败上抛（?）：保持 enabled=true，前端 showStatus('error') 提示用户重试
            reload_config_yaml(app, &p).await?;
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
/// controller `PUT /configs {path}` 热重载：重建 config.yaml 后通知 mihomo 重新加载，
/// root 进程常驻、免重启免再提权。停用状态直接跳过。
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

    // 重建 config.yaml（合并 subs/*.yaml 最新订阅）+ PUT /configs 热重载
    reload_config_yaml(app, &params).await?;

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

// ── 聚合菜单栏贡献（代理开启后向框架统一托盘贡献：开关/规则模式/节点切换） ──

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
/// 文案与界面 View.vue 一致；关闭（含 idle 常驻）返回空——菜单栏图标随之隐藏，保持干净。
fn build_proxy(app: &AppHandle) -> Vec<MenuEntry> {
    let state = app.state::<ProxyState>();
    if !state.enabled.load(Ordering::Relaxed) {
        return vec![];
    }
    // active：完整菜单（开启/规则/订阅/节点）
    let mut entries = vec![MenuEntry::CheckItem {
        id: "proxy_toggle".into(),
        label: "开启代理".into(),
        checked: true,
    }];
    let mode = crate::runtime::lock_or_recover(&state.run_params)
        .as_ref()
        .map(|p| p.mode.clone())
        .unwrap_or_else(|| "rule".to_string());
    let mode_label = match mode.as_str() {
        "global" => "全局",
        "direct" => "直连",
        _ => "规则",
    };
    entries.push(MenuEntry::Submenu {
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
    });
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
        // 开启代理 CheckItem（菜单仅 active 显示，点击 = 关闭转 idle）
        "proxy_toggle" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<ProxyState>();
                if let Err(e) = stop_core(&app, &state).await {
                    eprintln!("[proxy] 菜单关闭代理: {e}");
                }
                let _ = app.emit("proxy-enabled", false);
                crate::runtime::menubar::refresh(&app);
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

/// 解析 /proxies 响应取主分组（首个非 GLOBAL 的 Selector）及其节点列表：无 selector
/// （无订阅）返回 (None, [])——不回退 GLOBAL（其 all 仅含 DIRECT/REJECT 内置策略，非真实
/// 代理节点）。返回 (分组名, [(节点名, 是否当前选中)])。镜像 logic.ts pickMainGroup 语义。
fn parse_main_group(val: &Value) -> (Option<String>, Vec<(String, bool)>) {
    let Some(proxies) = val.get("proxies").and_then(|p| p.as_object()) else {
        return (None, Vec::new());
    };
    let main = proxies
        .iter()
        .find(|(k, e)| {
            e.get("type").and_then(|t| t.as_str()) == Some("Selector") && k.as_str() != "GLOBAL"
        })
        .map(|(_, v)| v);
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

/// app 启动复用上次退出遗留的 root mihomo（常驻方案下进程在 app 退出后仍跑）。
///
/// 先按 GET /version 验证 secret 是否匹配残留进程：
/// - 匹配 → 热重载 idle config（重置为 direct 直通，保证前端 enabled=false 与实际流量直通一致），
///   成功才标记 tun_active=true（下次开代理走热重载复用）。reload 失败属配置/瞬态，**不杀进程**
///   （保留常驻免提权），留给开代理时 active reload 处理。idle config 复用真实 mixed_port（与
///   `stop_core` 一致），避免 mixed-port 变更致 reload 失败。
/// - 不匹配（401）= 进程不可控（secret 启动时固化、reload 不生效）→ `stop_root` 清理僵尸，消除
///   不可控代理 + 开代理时的 401 回退噪音。best-effort：取消授权则保留，留给 `start_core` 回退兜底。
/// - controller 不可达（启动中/异常）→ 无法判定，保留不动。
///
/// 读 config.json 的 mixedPort/controllerPort/secret 连接残留 mihomo。
async fn reconnect_root_mihomo(app: &AppHandle) {
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
            // secret 不一致 = 不可控僵尸 → 清理
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
                Ok(()) => state.tun_active.store(true, Ordering::Relaxed),
                Err(e) => eprintln!("[proxy] 复用残留 mihomo、热重载 idle 失败: {e}"),
            }
        }
        Err(e) => eprintln!("[proxy] 残留 mihomo controller 不可达，跳过复用: {e}"),
    }
    crate::runtime::menubar::refresh(app);
}

/// 按 mihomo binary 完整路径 ps 查是否有 root 实例在跑（路径含 bundle-id 数据目录，唯一）。
fn root_mihomo_running(app: &AppHandle) -> bool {
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

/// 读 extensions/proxy/config.json 的 mixedPort/controllerPort/secret（前端 defineConfig 持久化），
/// 供热重载 idle 连接残留 mihomo。
fn read_controller_creds(app: &AppHandle) -> Option<(u16, u16, String)> {
    let path = crate::runtime::storage::ext_data_dir(app, "proxy")
        .ok()?
        .join("config.json");
    let text = std::fs::read_to_string(&path).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    let mixed_port = v.get("mixedPort")?.as_u64()? as u16;
    let controller_port = v.get("controllerPort")?.as_u64()? as u16;
    let secret = v.get("secret")?.as_str()?.to_string();
    Some((mixed_port, controller_port, secret))
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
            run_params: Mutex::new(None),
            tun_active: AtomicBool::new(false),
            menu_nodes: Mutex::new(Vec::new()),
            menu_subs: Mutex::new(Vec::new()),
        });
        crate::runtime::menubar::register(MenuBarContribution {
            title: "代理",
            build: Arc::new(build_proxy),
            on_event: Arc::new(on_proxy_event),
        });
        // app 启动预下载 Geo 数据库（首次使用经镜像下载，后续启动即时）+ 检测遗留 root
        // mihomo 复用。best-effort，不阻塞 setup。
        let app2 = app.clone();
        tauri::async_runtime::spawn(async move {
            let _ = core::ensure_geo_files(&app2).await;
            reconnect_root_mihomo(&app2).await;
        });
        Ok(())
    }
}
