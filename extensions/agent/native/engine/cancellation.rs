//! Per-session 取消注册中心。
//!
//! 每个 agent_run 创建一个 session_id（前端传入），注册到这里。
//! abort 命令按 session_id 查找并触发 CancellationToken。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::async_runtime::JoinHandle;
use tokio_util::sync::CancellationToken;

/// 单个 agent 会话的状态。
struct Session {
    /// 后台 task 的 handle（cancel 时 abort）
    handle: Option<JoinHandle<()>>,
    /// 取消令牌
    token: CancellationToken,
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
                },
            );
        token_clone
    }

    /// 在 register 之后调用，存入 task handle。
    /// 用于 abort 时强制终止 task。
    pub fn set_handle(&self, session_id: &str, handle: JoinHandle<()>) {
        if let Some(s) = self
            .sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(session_id)
        {
            s.handle = Some(handle);
        }
    }

    /// 取消并移除一个会话（用户点 abort 或会话自然结束时调用）。
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
}
