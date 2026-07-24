//! mihomo controller WebSocket 流桥接（/traffic /connections /logs）。
//!
//! 前端按需开流（传 `Channel<T>`），Rust 建 WS 长连接、逐帧 emit。停止走 `StreamRegistry`
//! 的 `CancellationToken`（前端调 `proxy_stop_stream` 或代理关闭时 `cancel_all` 触发）。
//! 与 agent `SessionRegistry` 同范式。WS 鉴权用 `?token={secret}` query（mihomo 支持）。
//!
//! 三条流均为本地回环（controller 固定 127.0.0.1），不经 `http::client()` 的 SSRF 防护
//! （与 controller.rs 一致）。连接失败静默退出（前端视图可见时才开流，无感重开）。

use crate::runtime::lock_or_recover;
use futures_util::StreamExt;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::ipc::Channel;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tokio_util::sync::CancellationToken;

/// 流 id 单例：同一时刻只有一份 traffic / connections / logs 流（key 固定，覆盖式重开）。
pub const ID_TRAFFIC: &str = "traffic";
pub const ID_CONNECTIONS: &str = "connections";
pub const ID_LOGS: &str = "logs";

/// /traffic 帧：上下行速率（bytes/s，mihomo 每秒推送一次）。
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct TrafficFrame {
    pub up: u64,
    pub down: u64,
}

/// /logs 帧：单行日志。
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct LogFrame {
    #[serde(rename = "type")]
    pub level: String,
    pub payload: String,
}

/// 流注册器（TauriState）：stream_id → CancellationToken。
///
/// register 时若 id 已存在，先 cancel 旧 token 再覆盖（防泄漏；前端覆盖式重开场景）。
/// cancel 触发对应 task 的 `select!` 分支退出，WS 连接随之 drop。
#[derive(Clone, Default)]
pub struct StreamRegistry {
    tokens: Arc<Mutex<HashMap<String, CancellationToken>>>,
}

impl StreamRegistry {
    /// 注册一条流，返回其 token。id 已存在时先 cancel 并替换。
    pub fn register(&self, id: String) -> CancellationToken {
        let token = CancellationToken::new();
        let mut guard = lock_or_recover(&self.tokens);
        if let Some(old) = guard.insert(id, token.clone()) {
            old.cancel();
        }
        token
    }

    /// 停止并移除一条流（前端离开子视图时调用）。
    pub fn cancel(&self, id: &str) -> bool {
        if let Some(token) = self
            .tokens
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id)
        {
            token.cancel();
            true
        } else {
            false
        }
    }

    /// 停止所有流（关代理 / 进程退出兜底，避免 idle 进程下残留 WS）。
    pub fn cancel_all(&self) {
        let map = std::mem::take(&mut *lock_or_recover(&self.tokens));
        for (_, token) in map {
            token.cancel();
        }
    }
}

/// 拼接 controller WS URL：`ws://127.0.0.1:{port}{path}[?qs]&token={secret}`。
/// path 形如 `/traffic`、`/logs?level=info`。
fn ws_url(port: u16, secret: &str, path: &str) -> String {
    let sep = if path.contains('?') { '&' } else { '?' };
    format!("ws://127.0.0.1:{port}{path}{sep}token={secret}")
}

/// /traffic 流：逐帧 emit `{ up, down }`。代理关闭/视图卸载时 token 触发退出。
pub async fn traffic_loop(
    port: u16,
    secret: &str,
    token: CancellationToken,
    channel: Channel<TrafficFrame>,
) {
    let url = ws_url(port, secret, "/traffic");
    let mut ws = match connect_async(url).await {
        Ok((s, _)) => s,
        Err(e) => {
            eprintln!("[proxy] traffic ws 连接失败: {e}");
            return;
        }
    };
    loop {
        tokio::select! {
            _ = token.cancelled() => break,
            msg = ws.next() => match msg {
                Some(Ok(Message::Text(t))) => {
                    if let Ok(frame) = serde_json::from_str::<TrafficFrame>(&t) {
                        let _ = channel.send(frame);
                    }
                }
                Some(Ok(_)) => {}
                _ => break,
            },
        }
    }
}

/// /connections 流：逐帧 emit 完整快照（含 connections 数组与累计总量，前端解析）。
/// `?interval=500` 提升 WS 推送频率（默认 1s 偏慢，连接列表刷新不及时）。
pub async fn connections_loop(
    port: u16,
    secret: &str,
    token: CancellationToken,
    channel: Channel<Value>,
) {
    let url = ws_url(port, secret, "/connections?interval=500");
    let mut ws = match connect_async(url).await {
        Ok((s, _)) => s,
        Err(e) => {
            eprintln!("[proxy] connections ws 连接失败: {e}");
            return;
        }
    };
    loop {
        tokio::select! {
            _ = token.cancelled() => break,
            msg = ws.next() => match msg {
                Some(Ok(Message::Text(t))) => {
                    if let Ok(v) = serde_json::from_str::<Value>(&t) {
                        let _ = channel.send(v);
                    }
                }
                Some(Ok(_)) => {}
                _ => break,
            },
        }
    }
}

/// /logs 流：逐帧 emit `{ type, payload }`。level 过滤由 mihomo 在连接时按 query 完成。
pub async fn logs_loop(
    port: u16,
    secret: &str,
    level: &str,
    token: CancellationToken,
    channel: Channel<LogFrame>,
) {
    let path = format!("/logs?level={level}");
    let url = ws_url(port, secret, &path);
    let mut ws = match connect_async(url).await {
        Ok((s, _)) => s,
        Err(e) => {
            eprintln!("[proxy] logs ws 连接失败: {e}");
            return;
        }
    };
    loop {
        tokio::select! {
            _ = token.cancelled() => break,
            msg = ws.next() => match msg {
                Some(Ok(Message::Text(t))) => {
                    if let Ok(frame) = serde_json::from_str::<LogFrame>(&t) {
                        let _ = channel.send(frame);
                    }
                }
                Some(Ok(_)) => {}
                _ => break,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_cancel() {
        let reg = StreamRegistry::default();
        let token = reg.register("traffic".into());
        assert!(!token.is_cancelled());
        assert!(reg.cancel("traffic"));
        assert!(token.is_cancelled());
        assert!(!reg.cancel("traffic")); // 已移除，重复 cancel 返回 false
    }

    #[test]
    fn cancel_unknown_returns_false() {
        let reg = StreamRegistry::default();
        assert!(!reg.cancel("missing"));
    }

    #[test]
    fn register_replaces_and_cancels_old() {
        let reg = StreamRegistry::default();
        let t1 = reg.register("logs".into());
        let t2 = reg.register("logs".into()); // 覆盖式重开
        assert!(t1.is_cancelled()); // 旧 token 被 cancel
        assert!(!t2.is_cancelled()); // 新 token 活跃
        reg.cancel("logs");
        assert!(t2.is_cancelled());
    }

    #[test]
    fn cancel_all_triggers_every() {
        let reg = StreamRegistry::default();
        let t1 = reg.register("a".into());
        let t2 = reg.register("b".into());
        reg.cancel_all();
        assert!(t1.is_cancelled());
        assert!(t2.is_cancelled());
        assert!(!reg.cancel("a")); // 全部清空
    }
}
