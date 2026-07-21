//! 代理菜单栏贡献：打开扩展 + 连接状态行（可点断开）。

use super::controller;
use super::lifecycle::{controller_endpoint, parse_current_node, stop_core, ProxyState};
use crate::runtime::menubar::{MenuBarContribution, MenuEntry};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

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

/// 菜单快照：打开扩展 + 连接状态。仅已连接时贡献。
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

/// 菜单点击：打开扩展 / 断开代理。
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

/// setup 内注册菜单贡献段。
pub(crate) fn register() {
    crate::runtime::menubar::register(MenuBarContribution {
        title: "代理",
        build: Arc::new(build_proxy),
        on_event: Arc::new(on_proxy_event),
    });
}
