//! 工具执行审批：oneshot channel + 全局 ApprovalManager。
//!
//! 设计：
//! - loop_runner 命中需审批的工具时，创建 oneshot channel
//! - 把 sender 存到全局 ApprovalManager（按 approval_id 索引）
//! - await receiver（在 select! 里同时等 cancel）
//! - 前端调 `agent_approve` command → ApprovalManager::resolve
//! - session abort 时 loop_runner 通过 select 退出，pending sender 被 drop

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
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
        Self {
            approved: false,
            always_approve: false,
        }
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
        // P4-rs1：毒锁恢复（与 shortcut.rs / cancellation.rs 一致）
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(tool_call_id, tx);
        rx
    }

    /// 前端审批完成后调用，唤醒 pending 的 loop_runner。
    /// 返回 false 表示 id 不存在（已超时或 session 已 abort）。
    pub fn resolve(&self, tool_call_id: &str, decision: Decision) -> bool {
        if let Some(tx) = self
            .pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(tool_call_id)
        {
            let _ = tx.send(decision);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_and_resolve() {
        let mgr = ApprovalManager::default();
        let rx = mgr.create("call_abc".to_string());

        let ok = mgr.resolve(
            "call_abc",
            Decision {
                approved: true,
                always_approve: false,
            },
        );
        assert!(ok);
        // 重复 resolve 已移除的 id 返回 false
        assert!(!mgr.resolve("call_abc", Decision::rejected()));

        let decision = rx.await.unwrap();
        assert!(decision.approved);
    }

    #[tokio::test]
    async fn resolve_unknown_id_returns_false() {
        let mgr = ApprovalManager::default();
        assert!(!mgr.resolve("missing", Decision::rejected()));
    }

    #[tokio::test]
    async fn resolve_after_receiver_dropped_returns_true_and_cleans_up() {
        // 模拟 session abort：loop_runner 持有的 receiver 被 drop，sender 仍留在 map 中。
        // 前端 resolve 应安全完成：移除 sender、send 失败被静默吞掉、返回 true（id 曾存在），
        // 且后续 resolve 返回 false（map 已清理，避免泄漏）。
        let mgr = ApprovalManager::default();
        let rx = mgr.create("call_x".to_string());
        drop(rx);
        assert!(mgr.resolve("call_x", Decision::rejected()));
        assert!(!mgr.resolve("call_x", Decision::rejected()));
    }
}
