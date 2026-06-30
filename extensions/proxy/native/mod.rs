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

/// 代理运行状态：enabled（流量是否被代理）+ tun_active（root mihomo 进程是否在跑，常驻）
/// + run_params（最近一次 active 参数，供热重载/复用）。
pub struct ProxyState {
    pub enabled: AtomicBool,
    pub run_params: Mutex<Option<RunParams>>,
    pub tun_active: AtomicBool,
    /// 菜单栏状态行当前节点名缓存；由 refresh_proxy_menu 异步拉取填充。
    pub current_node: Mutex<Option<String>>,
}

/// 启用/停用代理（统一 TUN 模式：root mihomo 常驻 + 热重载 active/idle）。
///
/// 启用：`ensure_root_mihomo`（首次提权 spawn，之后幂等）+ 热重载 active config；常驻后免再提权。
/// 停用：热重载 idle config（mode=direct + 无 tun 段）→ mihomo 撤销 utun、流量直通，进程保留。
/// run 参数（端口/secret/mode）由前端 config 传入，Rust 仅消费不回读 plugin-store。
/// 菜单栏图标常驻：状态行显示「已连接：节点」/「未连接」，控制逻辑全部在扩展面板（菜单不重复）。
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
/// root mihomo 常驻：优先热重载 idle config（mode=direct + 无 tun 段）→ mihomo 撤销 utun、
/// 流量直通，进程保留以便下次免提权恢复。
/// 热重载失败（mihomo 崩溃/controller 卡死致 127.0.0.1:9090 不可达）回退强杀：进程已退出
/// 则直接重置状态（TUN 已由 OS 回收，免提权）；进程仍跑（卡死）则 stop_root 强杀释放 TUN，
/// 保关闭可靠性。无论优雅还是强杀，最终 enabled=false——用户「关闭代理」意图必须达成，
/// 不可卡死在无法关闭态。
async fn stop_core(app: &AppHandle, state: &ProxyState) -> Result<(), String> {
    if state.tun_active.load(Ordering::Relaxed) {
        let idle = state.run_params.lock().map_err(|e| e.to_string())?.clone();
        if let Some(mut p) = idle {
            p.mode = "direct".into();
            p.tun = false;
            // 优雅关闭失败：controller 不可达（进程崩溃/卡死）。
            // 进程仍跑（卡死）→ stop_root 强杀释放 TUN；进程已退出 → 直接重置（免提权）。
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
/// 中途下载失败：文件已删，下次 ensure_bin 自动重试（前端进面板触发或开代理触发）。
#[tauri::command]
pub async fn proxy_update_core(app: AppHandle, state: State<'_, ProxyState>) -> Result<(), String> {
    let was_enabled = state.enabled.load(Ordering::Relaxed);
    let params = state.run_params.lock().map_err(|e| e.to_string())?.clone();
    // 停代理 + kill root 进程（替换 binary 必须先 kill）。失败上抛：保留旧 binary 不动。
    if state.tun_active.load(Ordering::Relaxed) {
        tun::stop_root(&app)?;
        state.tun_active.store(false, Ordering::Relaxed);
    }
    state.enabled.store(false, Ordering::Relaxed);
    // 删旧 binary + version → ensure_bin 走 fetch_latest_asset 重下最新
    core::remove_core_files(&app)?;
    core::ensure_bin(&app).await?;
    // 恢复启用状态（若之前在跑）。start_core 会重新 spawn_root + 提权
    if was_enabled {
        if let Some(p) = params {
            start_core(&app, &state, p).await?;
        }
    }
    Ok(())
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

/// PUT /proxies/{group} → 在 selector 分组选择节点。成功后刷新菜单状态行节点名。
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

// ── 聚合菜单栏贡献（打开扩展 + 连接状态行；控制逻辑全部在扩展面板，菜单不重复） ──

/// 拉取当前选中节点名刷新菜单状态行（best-effort，controller 不可达时保留上次缓存）。
async fn refresh_proxy_menu(app: &AppHandle) {
    let state = app.state::<ProxyState>();
    if let Ok((base, secret)) = controller_endpoint(&state) {
        if let Ok(val) = controller::get_proxies(&base, &secret).await {
            *crate::runtime::lock_or_recover(&state.current_node) = parse_current_node(&val);
        }
    }
    crate::runtime::menubar::refresh(app);
}

/// 菜单快照：打开扩展（打开面板）+ 连接状态（CheckItem 可点断开）。
/// 仅已连接时贡献——断开后图标隐藏（保持菜单栏干净），重连走扩展面板。
/// 开关/模式/订阅/节点切换/测速等完整控制仍在扩展面板。
fn build_proxy(app: &AppHandle) -> Vec<MenuEntry> {
    let state = app.state::<ProxyState>();
    if !state.enabled.load(Ordering::Relaxed) {
        return vec![];
    }
    let label = match crate::runtime::lock_or_recover(&state.current_node)
        .clone()
        .filter(|n| !n.is_empty())
    {
        Some(n) => format!("已连接：{n}"),
        None => "已连接".to_string(),
    };
    vec![
        MenuEntry::Item {
            id: "proxy_open".into(),
            label: "打开扩展".into(),
            enabled: true,
        },
        MenuEntry::CheckItem {
            id: "proxy_toggle".into(),
            label,
            checked: true,
        },
    ]
}

/// 菜单点击分派：打开扩展（打开面板）/ 连接状态（断开代理 → 图标隐藏）。
fn on_proxy_event(app: &AppHandle, id: &str) {
    match id {
        "proxy_open" => {
            let app2 = app.clone();
            let _ = app.run_on_main_thread(move || {
                crate::runtime::window::show_main(&app2);
                let _ = app2.emit("open-module", "proxy");
            });
        }
        "proxy_toggle" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<ProxyState>();
                if let Err(e) = stop_core(&app, &state).await {
                    eprintln!("[proxy] 菜单关闭代理失败: {e}");
                    return;
                }
                let _ = app.emit("proxy-enabled", false);
                crate::runtime::menubar::refresh(&app);
            });
        }
        _ => {}
    }
}

/// 解析 /proxies 响应取主分组（首个非 GLOBAL 的 Selector）的当前选中节点名（`now`）。
/// 无 selector（无订阅）返回 None。镜像 logic.ts pickMainGroup 语义。
fn parse_current_node(val: &Value) -> Option<String> {
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

/// 读 extensions/proxy/config.json 构造 RunParams（mode 缺省 rule，tun 恒 true）。
/// `read_controller_creds` 委托此函数；前端 defineConfig 持久化的参数由此读回。
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

/// 读 config.json 的 controller 凭据（mixedPort/controllerPort/secret），供热重载 idle 连接残留 mihomo。
fn read_controller_creds(app: &AppHandle) -> Option<(u16, u16, String)> {
    let p = read_run_params(app)?;
    Some((p.mixed_port, p.controller_port, p.secret))
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
            current_node: Mutex::new(None),
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
