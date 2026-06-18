use serde::{Deserialize, Serialize};

use crate::runtime::llm::parser::FinalizedToolCall;

/// LLM 协议层消息（OpenAI 兼容格式）。
///
/// agent / translate 扩展均使用此类型。
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LlmMessage {
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// assistant 消息携带的 tool_calls（role=assistant 且触发了工具时）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<LlmToolCall>>,
    /// role=tool 时必填，对应 tool_call.id
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl LlmMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: "user".into(), content: Some(content.into()), tool_calls: None, tool_call_id: None }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: "assistant".into(), content: Some(content.into()), tool_calls: None, tool_call_id: None }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self { role: "system".into(), content: Some(content.into()), tool_calls: None, tool_call_id: None }
    }

    /// role=tool 的结果回灌消息。
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".into(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}

/// OpenAI 兼容的 tool_call 结构（用于回灌 assistant 消息）。
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LlmToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: LlmToolCallFunction,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LlmToolCallFunction {
    pub name: String,
    pub arguments: String,
}

impl From<&FinalizedToolCall> for LlmToolCall {
    fn from(c: &FinalizedToolCall) -> Self {
        Self {
            id: c.id.clone(),
            kind: "function".into(),
            function: LlmToolCallFunction {
                name: c.name.clone(),
                arguments: c.arguments.to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llm_message_helpers() {
        let u = LlmMessage::user("hi");
        assert_eq!(u.role, "user");
        assert_eq!(u.content.as_deref(), Some("hi"));
        assert!(u.tool_calls.is_none());

        let t = LlmMessage::tool_result("call_1", "result");
        assert_eq!(t.role, "tool");
        assert_eq!(t.tool_call_id.as_deref(), Some("call_1"));
    }
}
