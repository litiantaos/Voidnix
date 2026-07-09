//! Agent 历史消息裁剪（agent 唯一消费者，下沉自 runtime/llm/security.rs）。

use crate::runtime::llm::LlmMessage;

/// 历史消息条数硬上限
const MAX_CONVERSATION_MESSAGES: usize = 100;

/// 裁剪对话历史至硬上限。保留 system 消息（若存在），从尾部保留最近消息。
///
/// H15：边界安全——裁剪点可能落在 assistant(tool_calls) 与 role=tool 之间，
/// 导致首条是孤立 role=tool。OpenAI API 要求 tool 消息前必须有对应的
/// assistant(tool_calls)，否则返 400。裁剪后向前剥离孤立 tool 消息，
/// 再剥离因 tool 被剥离而悬空的 assistant(tool_calls)（无对应 tool 结果）。
pub fn trim_conversation(messages: &[LlmMessage]) -> Vec<LlmMessage> {
    if messages.len() <= MAX_CONVERSATION_MESSAGES {
        return messages.to_vec();
    }
    let system_msg = messages.iter().find(|m| m.role == "system").cloned();
    let skip = messages.len() - (MAX_CONVERSATION_MESSAGES - 1);
    let mut trimmed: Vec<LlmMessage> = messages.iter().skip(skip).cloned().collect();
    // 向前剥离不完整的开头：孤立 role=tool 或无 tool 结果跟随的 assistant(tool_calls)
    loop_trim_boundary(&mut trimmed);
    if let Some(sys) = system_msg {
        if trimmed.first().map(|m| m.role.as_str()) != Some("system") {
            trimmed.insert(0, sys);
        }
    }
    trimmed
}

/// 剥离开头不完整的工具调用轮次。
fn loop_trim_boundary(trimmed: &mut Vec<LlmMessage>) {
    loop {
        let strip = match trimmed.first() {
            // 孤立 tool（其 assistant tool_calls 被裁掉）
            Some(f) if f.role == "tool" => true,
            // assistant(tool_calls) 但下一条不是 tool（tool 结果被裁掉）
            Some(f)
                if f.role == "assistant"
                    && f.tool_calls.as_ref().is_some_and(|tc| !tc.is_empty())
                    && !next_is_tool(trimmed) =>
            {
                true
            }
            _ => false,
        };
        if strip {
            trimmed.remove(0);
        } else {
            break;
        }
    }
}

/// 第二条消息（index=1）是否为 tool 结果。
fn next_is_tool(trimmed: &[LlmMessage]) -> bool {
    trimmed.get(1).map(|m| m.role == "tool").unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_conversation_preserves_system_msg() {
        let mut msgs = vec![LlmMessage::system("sys")];
        for i in 0..150 {
            msgs.push(LlmMessage::user(format!("u{}", i)));
        }
        let trimmed = trim_conversation(&msgs);
        assert!(trimmed.len() <= MAX_CONVERSATION_MESSAGES);
        assert_eq!(trimmed[0].role, "system");
    }

    #[test]
    fn trim_conversation_short_input_unchanged() {
        let msgs = vec![LlmMessage::user("hi"), LlmMessage::assistant("hello")];
        let trimmed = trim_conversation(&msgs);
        assert_eq!(trimmed.len(), 2);
    }

    #[test]
    fn trim_strips_orphan_tool_at_start() {
        // H15：裁剪点落在 assistant(tool_calls) 之后，导致首条是孤立 tool
        let mut msgs = vec![LlmMessage::system("sys")];
        // 填充到 100+ 条
        for i in 0..110 {
            msgs.push(LlmMessage::user(format!("u{}", i)));
            msgs.push(LlmMessage::assistant(format!("a{}", i)));
        }
        // 手动模拟裁剪后的孤立情况：构造一条 tool 在 trimmed 开头，
        // 后接正常的 user/assistant 对
        let mut simulated = vec![LlmMessage::tool_result("orphan", "x")];
        simulated.extend(msgs.iter().skip(msgs.len() - 5).cloned());
        // 验证 loop_trim_boundary 能剥离孤立 tool
        loop_trim_boundary(&mut simulated);
        assert_ne!(simulated.first().map(|m| m.role.as_str()), Some("tool"));
    }

    #[test]
    fn trim_strips_orphan_assistant_tool_calls_at_start() {
        // H15：assistant 带 tool_calls 但下一条不是 tool（结果被裁）
        use crate::runtime::llm::types::{LlmToolCall, LlmToolCallFunction};
        let mut orphan = LlmMessage::assistant("call");
        orphan.tool_calls = Some(vec![LlmToolCall {
            id: "c".into(),
            kind: "function".into(),
            function: LlmToolCallFunction {
                name: "run_command".into(),
                arguments: "{}".into(),
            },
        }]);
        let mut simulated = vec![orphan, LlmMessage::user("next")]; // 第二条不是 tool
        loop_trim_boundary(&mut simulated);
        assert_eq!(simulated.first().map(|m| m.role.as_str()), Some("user"));
    }
}
