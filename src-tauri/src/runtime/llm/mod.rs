pub mod client;
pub mod parser;
pub mod types;

// 便捷 re-export：扩展可直接 `use crate::runtime::llm::{self, LlmMessage}`
// runtime/llm = client.rs + parser.rs + types.rs（security 已溶解入 client，§1.1）
// parse_scheme_host 原语已下沉 http.rs（SSRF 校验单一源），此处保留 re-export 供扩展兼容消费
pub use crate::http::parse_scheme_host;
pub use client::{openai_request_once, stream_openai_request, validate_ai_request, StreamConfig};
pub use types::{LlmMessage, LlmToolCall};
