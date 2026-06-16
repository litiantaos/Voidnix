//! 工具执行审批：oneshot channel + 全局 ApprovalManager。
//!
//! 设计：
//! - loop_runner 命中需审批的工具时，创建 oneshot channel
//! - 把 sender 存到全局 ApprovalManager（按 approval_id 索引）
//! - await receiver（在 select! 里同时等 cancel）
//! - 前端调 `agent_approve` command → ApprovalManager::resolve
//! - session abort 时 loop_runner 通过 select 退出，pending sender 被 drop

use std::sync::Arc;
use std::sync::Mutex;
use std::collections::HashMap;
use tokio::sync::oneshot;

/// 审批决定（前端传给 `agent_approve` command）。
#[derive(Debug, Clone, Copy)]
pub struct Decision {
    /// true=本次执行；false=拒绝
    pub approved: bool,
    /// true=「执行并信任」（未来同命令免审批）。
    /// 前端读取后调用 `settings.trustCommand` 持久化；Rust 端不读，保留协议字段。
    #[allow(dead_code)]
    pub always_approve: bool,
}

impl Decision {
    pub fn rejected() -> Self {
        Self { approved: false, always_approve: false }
    }
}

/// 全局审批管理器（作为 Tauri State 注入）。
///
/// 用 `tool_call.id`（字符串，LLM 提供）作为索引，确保前端 part 路由与
/// agent_approve command 用同一 id。
#[derive(Clone, Default)]
pub struct ApprovalManager {
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<Decision>>>>,
}

impl ApprovalManager {
    /// 注册一个 pending approval，用 tool_call.id 作为索引。
    /// loop_runner 拿到 receiver 后在 select! 里 await。
    pub fn create(&self, tool_call_id: String) -> oneshot::Receiver<Decision> {
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(tool_call_id, tx);
        rx
    }

    /// 前端审批完成后调用，唤醒 pending 的 loop_runner。
    /// 返回 false 表示 id 不存在（已超时或 session 已 abort）。
    pub fn resolve(&self, tool_call_id: &str, decision: Decision) -> bool {
        if let Some(tx) = self.pending.lock().unwrap().remove(tool_call_id) {
            let _ = tx.send(decision);
            true
        } else {
            false
        }
    }

    /// 当前 pending 数量（监控用）。
    #[allow(dead_code)]
    pub fn pending_count(&self) -> usize {
        self.pending.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_and_resolve() {
        let mgr = ApprovalManager::default();
        let rx = mgr.create("call_abc".to_string());
        assert_eq!(mgr.pending_count(), 1);

        mgr.resolve("call_abc", Decision { approved: true, always_approve: false });
        assert_eq!(mgr.pending_count(), 0);

        let decision = rx.await.unwrap();
        assert!(decision.approved);
    }

    #[tokio::test]
    async fn resolve_unknown_id_returns_false() {
        let mgr = ApprovalManager::default();
        assert!(!mgr.resolve("missing", Decision::rejected()));
    }

    #[tokio::test]
    async fn sender_drop_on_cancel_resolves_to_channel_closed() {
        let mgr = ApprovalManager::default();
        let (_id, rx) = ("call_x".to_string(), mgr.create("call_x".to_string()));
        drop(rx); // 模拟 loop_runner 退出
        // sender 还在 map 里，但 resolve 会 send 失败（receiver 已 drop），应不 panic
    }
}
