pub mod client;
pub mod parser;
pub mod security;
pub mod types;

// 便捷 re-export：扩展可直接 `use crate::runtime::llm::{self, LlmMessage}`
pub use client::{stream_openai_request, StreamConfig};
pub use security::{parse_scheme_host, trim_conversation, validate_ai_request};
pub use types::{LlmMessage, LlmToolCall};
