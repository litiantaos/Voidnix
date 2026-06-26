//! Agent 框架层：工具循环、取消、事件流。
//!
//! 架构层次：
//! - `tool_registry`：AgentTool trait + 注册中心
//! - `loop_runner`：后台 task 主循环（LLM ↔ 工具调用）
//! - `cancellation`：per-session CancellationToken + SessionRegistry

pub mod cancellation;
pub mod loop_runner;
pub mod secret_scrub;
pub mod tool_registry;
pub mod trim;

use serde::Serialize;

/// Agent 通过 Channel 推给前端的增量事件。
///
/// 命名约定（前端 TS 类型对齐）：
/// - `TextDelta` → 累积成 assistant message 的 text part
/// - `ToolCallStart` / `ToolCallArgs` → 新建 tool_call part
/// - `ToolResult` → 更新对应 tool_call part
/// - `Completed` / `Error` → 结束当前 agent run
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AgentEvent {
    /// LLM 输出的文本增量（assistant 文本流）
    TextDelta { text: String },
    /// 工具调用开始（拿到 tool_call_id + 工具名）
    ToolCallStart { id: String, name: String },
    /// 工具调用参数完整到达（已 parse 的 JSON）
    ToolCallArgs { id: String, args: serde_json::Value },
    /// 工具执行结果（已净化 secret）
    ToolResult {
        id: String,
        ok: bool,
        output: String,
    },
    /// 本轮 agent run 完成（无更多工具调用）
    Completed,
    /// 错误终止（附人类可读消息）
    Error { message: String },
}
