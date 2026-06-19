//! Agent 历史消息裁剪（agent 唯一消费者，下沉自 runtime/llm/security.rs，§1.1）。

use crate::runtime::llm::LlmMessage;

/// 历史消息条数硬上限
const MAX_CONVERSATION_MESSAGES: usize = 100;

/// 裁剪对话历史至硬上限。保留 system 消息（若存在），从尾部保留最近消息。
pub fn trim_conversation(messages: &[LlmMessage]) -> Vec<LlmMessage> {
    if messages.len() <= MAX_CONVERSATION_MESSAGES {
        return messages.to_vec();
    }
    let system_msg = messages.iter().find(|m| m.role == "system").cloned();
    let skip = messages.len() - (MAX_CONVERSATION_MESSAGES - 1);
    let mut trimmed: Vec<LlmMessage> = messages.iter().skip(skip).cloned().collect();
    if let Some(sys) = system_msg {
        if trimmed.first().map(|m| m.role.as_str()) != Some("system") {
            trimmed.insert(0, sys);
        }
    }
    trimmed
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
}
