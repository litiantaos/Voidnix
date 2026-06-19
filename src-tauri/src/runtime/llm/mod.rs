pub mod client;
pub mod parser;
pub mod types;

// 便捷 re-export：扩展可直接 `use crate::runtime::llm::{self, LlmMessage}`
// runtime/llm = client.rs + parser.rs + types.rs（security 已溶解入 client，§1.1）
pub use client::{parse_scheme_host, stream_openai_request, validate_ai_request, StreamConfig};
pub use types::{LlmMessage, LlmToolCall};
