//! 代理菜单栏贡献：打开扩展 + 连接状态行（可点切换连接）。显隐由 menubarVisible
//! 常显开关控制（替代原「已连接才显示」逻辑），见 config.ts 与 set_proxy_menubar_visible。

use super::controller;
use super::core;
use super::lifecycle::{
    controller_endpoint, parse_current_node, read_run_params, start_core, stop_core, tun_confirmed,
    ProxyState,
};
use crate::runtime::menubar::{MenuBarContribution, MenuEntry};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

/// 菜单栏连接进行中守卫：连接含提权安装等慢路径（密码弹窗期间用户可能重开菜单再点），
/// 二次点击直接忽略（镜像 awake 的 engaging 守卫，防 osascript 授权对话框叠加）。
static CONNECTING: AtomicBool = AtomicBool::new(false);

/// 拉取当前选中节点名刷新菜单状态行（best-effort）。
pub(crate) async fn refresh_proxy_menu(app: &AppHandle) {
    let state = app.state::<ProxyState>();
    if let Ok((base, secret)) = controller_endpoint(&state) {
        if let Ok(val) = controller::get_proxies(&base, &secret).await {
            *crate::runtime::lock_or_recover(&state.current_node) = parse_current_node(&val);
        }
    }
    crate::runtime::menubar::refresh(app);
}

/// 菜单快照：menubar_visible 开启时恒贡献「打开扩展」+ 连接状态 CheckItem
/// （勾选态反映 enabled；已连接显示当前节点，未连接显示「开启代理」，点击切换连接）；
/// 关闭时返回空段。
fn build_proxy(app: &AppHandle) -> Vec<MenuEntry> {
    let state = app.state::<ProxyState>();
    if !state.menubar_visible.load(Ordering::Relaxed) {
        return vec![];
    }
    let enabled = state.enabled.load(Ordering::Relaxed);
    let label = if enabled {
        match crate::runtime::lock_or_recover(&state.current_node)
            .clone()
            .filter(|n| !n.is_empty())
        {
            Some(n) => format!("已连接：{n}"),
            None => "已连接".to_string(),
        }
    } else {
        "开启代理".to_string()
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
            checked: enabled,
        },
    ]
}

/// 菜单点击：打开扩展 / 切换连接。断开走 stop_core（热重载 idle）；连接走 start_core，
/// 未过首启流程（未确认 TUN 告知 / 核心未就绪 / 无运行参数）回退打开扩展走完整流程。
fn on_proxy_event(app: &AppHandle, id: &str) {
    match id {
        "proxy_open" => open_extension(app),
        "proxy_toggle" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<ProxyState>();
                if state.enabled.load(Ordering::Relaxed) {
                    if let Err(e) = stop_core(&app, &state).await {
                        // 经 proxy-status 事件反馈（前端面板监听显示 toast），不再只 eprintln 静默
                        let _ = app.emit(
                            "proxy-status",
                            super::lifecycle::ProxyStatus {
                                kind: "error".into(),
                                msg: e,
                            },
                        );
                        return;
                    }
                    let _ = app.emit("proxy-enabled", false);
                    crate::runtime::menubar::refresh(&app);
                    return;
                }
                if !tun_confirmed(&app) || !core::core_status(&app).downloaded {
                    open_extension(&app);
                    return;
                }
                let Some(params) = read_run_params(&app) else {
                    open_extension(&app);
                    return;
                };
                if CONNECTING.swap(true, Ordering::Relaxed) {
                    return; // 连接进行中（提权弹窗等慢路径），忽略二次点击
                }
                // 与 set_proxy_enabled 开启路径对齐：start → refresh → emit → 拉节点名
                let result = start_core(&app, &state, params).await;
                CONNECTING.store(false, Ordering::Relaxed);
                if let Err(e) = result {
                    let _ = app.emit(
                        "proxy-status",
                        super::lifecycle::ProxyStatus {
                            kind: "error".into(),
                            msg: e,
                        },
                    );
                    return;
                }
                crate::runtime::menubar::refresh(&app);
                let _ = app.emit("proxy-enabled", true);
                refresh_proxy_menu(&app).await;
            });
        }
        _ => {}
    }
}

/// emit open-extension 打开代理视图。show 由前端 listener 控制（wasVisible=false 时
/// setActiveExtension → rAF → showWindow），避免 Rust 立即 show 时 webview 仍为旧视图的闪现。
fn open_extension(app: &AppHandle) {
    let was_visible = crate::runtime::shortcut::is_window_visible();
    let _ = app.emit(
        "open-extension",
        serde_json::json!({ "id": "proxy", "wasVisible": was_visible }),
    );
}

/// setup 内注册菜单贡献段。
pub(crate) fn register() {
    crate::runtime::menubar::register(MenuBarContribution {
        title: "代理",
        build: Arc::new(build_proxy),
        on_event: Arc::new(on_proxy_event),
    });
}
