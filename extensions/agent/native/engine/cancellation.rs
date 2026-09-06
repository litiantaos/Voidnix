//! Per-session 取消注册中心。
//!
//! 每个 agent_run 创建一个 session_id（前端传入），注册到这里。
//! abort 命令按 session_id 查找并触发 CancellationToken。

use crate::runtime::lock_or_recover;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::async_runtime::JoinHandle;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

/// 单个 agent 会话的状态。
struct Session {
    /// 后台 task 的 handle（cancel 时 abort）
    handle: Option<JoinHandle<()>>,
    /// 取消令牌
    token: CancellationToken,
    /// 等待用户审批的 tool_call（call_id → 决策 sender）。
    /// session 被 cancel/unregister 移除时 sender 随之 drop → loop 侧 rx err → 视为拒绝。
    pending_approvals: HashMap<String, oneshot::Sender<bool>>,
}

/// 全局 session 注册器（作为 Tauri State 注入）。
///
/// 内部用 `Arc<Mutex<...>>` 共享，SessionRegistry 本身实现 Clone（cheap clone）。
/// P4-rs1：lock 失败时 unwrap_or_else 恢复（与 shortcut.rs lock_or_recover 一致）。
#[derive(Clone, Default)]
pub struct SessionRegistry {
    sessions: Arc<Mutex<HashMap<String, Session>>>,
}

impl SessionRegistry {
    /// 注册一个新会话，返回 token 的 clone 供 loop_runner 使用。
    pub fn register(&self, session_id: String, token: CancellationToken) -> CancellationToken {
        let token_clone = token.clone();
        self.sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(
                session_id,
                Session {
                    handle: None,
                    token,
                    pending_approvals: HashMap::new(),
                },
            );
        token_clone
    }

    /// 在 register 之后调用，存入 task handle。
    /// 用于 abort 时强制终止 task。
    /// 若 session 已不在（register 与 set_handle 之间被 cancel/unregister），abort handle 并返回 false。
    pub fn set_handle(&self, session_id: &str, handle: JoinHandle<()>) -> bool {
        let mut map = lock_or_recover(&self.sessions);
        if let Some(s) = map.get_mut(session_id) {
            s.handle = Some(handle);
            true
        } else {
            // 竞态窗口：abort 已 remove session，强制终止刚 spawn 的 task
            handle.abort();
            false
        }
    }

    /// 取消并移除一个会话（用户点 abort）。
    pub fn cancel(&self, session_id: &str) -> bool {
        if let Some(mut session) = self
            .sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(session_id)
        {
            session.token.cancel();
            if let Some(handle) = session.handle.take() {
                handle.abort();
            }
            true
        } else {
            false
        }
    }

    /// 自然结束时移除会话（不 cancel token / 不 abort handle）。
    /// 与 `cancel` 互斥：任一方先 remove 后另一方 no-op。
    pub fn unregister(&self, session_id: &str) -> bool {
        self.sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(session_id)
            .is_some()
    }

    /// 挂起一个待审批 tool_call 的决策通道（loop_runner 审批门调用）。
    /// session 不存在（已 cancel/结束）时 sender 直接 drop → loop 侧视为拒绝。
    pub fn register_approval(&self, session_id: &str, call_id: &str, tx: oneshot::Sender<bool>) {
        let mut map = lock_or_recover(&self.sessions);
        if let Some(s) = map.get_mut(session_id) {
            s.pending_approvals.insert(call_id.to_string(), tx);
        }
    }

    /// 回填审批决策（agent_approve 命令调用）。存在等待项并完成投递返回 true。
    pub fn respond(&self, session_id: &str, call_id: &str, approved: bool) -> bool {
        let mut map = lock_or_recover(&self.sessions);
        let Some(s) = map.get_mut(session_id) else {
            return false;
        };
        match s.pending_approvals.remove(call_id) {
            Some(tx) => {
                // loop 侧已超时/取消（rx dropped）时 send err，忽略
                let _ = tx.send(approved);
                true
            }
            None => false,
        }
    }

    /// 是否存在进行中的 run（WebContent 重载守卫消费：run 进行中延迟 webview 重载）。
    pub fn has_active(&self) -> bool {
        !self
            .sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_empty()
    }

    /// 当前注册会话数（测试 / 诊断用）。
    #[cfg(test)]
    fn len(&self) -> usize {
        self.sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_cancel() {
        let reg = SessionRegistry::default();
        let token = reg.register("s1".to_string(), CancellationToken::new());
        assert!(!token.is_cancelled());

        assert!(reg.cancel("s1"));
        assert!(token.is_cancelled());
        // 重复 cancel 已移除的 session 返回 false
        assert!(!reg.cancel("s1"));
    }

    #[test]
    fn cancel_unknown_session_returns_false() {
        let reg = SessionRegistry::default();
        assert!(!reg.cancel("missing"));
    }

    #[test]
    fn has_active_reflects_lifecycle() {
        let reg = SessionRegistry::default();
        assert!(!reg.has_active());
        reg.register("s1".to_string(), CancellationToken::new());
        assert!(reg.has_active());
        reg.unregister("s1");
        assert!(!reg.has_active());
    }

    #[test]
    fn unregister_removes_without_cancelling_token() {
        let reg = SessionRegistry::default();
        let token = reg.register("s1".to_string(), CancellationToken::new());
        assert!(reg.unregister("s1"));
        assert!(!token.is_cancelled());
        assert_eq!(reg.len(), 0);
        // 已移除后重复 unregister / cancel 均为 false
        assert!(!reg.unregister("s1"));
        assert!(!reg.cancel("s1"));
    }

    #[test]
    fn unregister_after_cancel_is_noop() {
        let reg = SessionRegistry::default();
        reg.register("s1".to_string(), CancellationToken::new());
        assert!(reg.cancel("s1"));
        assert!(!reg.unregister("s1"));
        assert_eq!(reg.len(), 0);
    }

    #[tokio::test]
    async fn approval_register_and_respond() {
        let reg = SessionRegistry::default();
        reg.register("s1".to_string(), CancellationToken::new());

        let (tx, rx) = oneshot::channel();
        reg.register_approval("s1", "call1", tx);
        // 重复回填同一 call：第二次无等待项
        assert!(reg.respond("s1", "call1", true));
        assert!(!reg.respond("s1", "call1", true));
        assert_eq!(rx.await, Ok(true));

        // 未知 call / 未知 session
        let (tx2, rx2) = oneshot::channel();
        reg.register_approval("s1", "call2", tx2);
        assert!(!reg.respond("missing", "call2", false));
        assert!(!reg.respond("s1", "missing", false));
        // 无人回填的等待项仍在（loop 侧继续等）；drop rx2 防 clippy 未读警告
        drop(rx2);
    }

    #[tokio::test]
    async fn approval_session_removed_denies_pending() {
        // cancel/unregister 移除 session → pending sender drop → loop 侧 rx err（视为拒绝）
        let reg = SessionRegistry::default();
        reg.register("s1".to_string(), CancellationToken::new());

        let (tx, rx) = oneshot::channel();
        reg.register_approval("s1", "call1", tx);
        assert!(reg.cancel("s1"));
        assert!(rx.await.is_err());

        // session 已不存在时挂审批：sender 直接 drop，loop 侧同样收到 err
        let (tx2, rx2) = oneshot::channel();
        reg.register_approval("s1", "call1", tx2);
        assert!(rx2.await.is_err());
    }

    #[tokio::test]
    async fn set_handle_after_cancel_aborts_task() {
        let reg = SessionRegistry::default();
        reg.register("s1".to_string(), CancellationToken::new());
        assert!(reg.cancel("s1"));

        // register→set_handle 竞态：session 已 remove，set_handle 必须 abort 并返回 false
        let handle = tauri::async_runtime::spawn(async {
            // 若未 abort 会挂起至 test timeout；abort 后 JoinHandle 以 cancel 结束
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            }
        });
        assert!(!reg.set_handle("s1", handle));
        // 给 runtime 一点时间处理 abort
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        assert_eq!(reg.len(), 0);
    }

    #[tokio::test]
    async fn set_handle_attaches_and_cancel_aborts() {
        let reg = SessionRegistry::default();
        let token = reg.register("s1".to_string(), CancellationToken::new());
        let handle = tauri::async_runtime::spawn(async {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            }
        });
        assert!(reg.set_handle("s1", handle));
        assert!(reg.cancel("s1"));
        assert!(token.is_cancelled());
        assert_eq!(reg.len(), 0);
    }
}
