use crate::runtime::llm::parser::{ChoiceDelta, FinalizedToolCall, ToolCallAccumulator};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;
use tauri::Emitter;

/// ─── 安全常量 ───────────────────────────────────────────
pub const MAX_SSE_BUFFER: usize = 1_048_576; // 1 MiB — 防止无界 buffer 增长
pub const MAX_MESSAGE_CONTENT_LEN: usize = 32_768; // 32 KiB — 单条消息上限
pub const MAX_CONVERSATION_MESSAGES: usize = 100; // 历史消息条数硬上限

/// ─── SSRF 防护 ──────────────────────────────────────────
/// 手动解析 URL 提取 scheme + host，不依赖 url crate
pub fn parse_scheme_host(raw: &str) -> Option<(&str, &str)> {
    let s = raw.trim();
    let scheme_end = s.find("://")?;
    let scheme = &s[..scheme_end];
    let rest = &s[scheme_end + 3..];
    let host_end = rest
        .find(['/', '?', '#'])
        .unwrap_or(rest.len());
    let host_port = &rest[..host_end];
    let host = host_port.split(':').next()?;
    if scheme.is_empty() || host.is_empty() {
        return None;
    }
    Some((scheme, host))
}

const BLOCKED_HOST_PREFIXES: &[&str] = &[
    "127.",
    "10.",
    "192.168.",
    "172.16.",
    "172.17.",
    "172.18.",
    "172.19.",
    "172.20.",
    "172.21.",
    "172.22.",
    "172.23.",
    "172.24.",
    "172.25.",
    "172.26.",
    "172.27.",
    "172.28.",
    "172.29.",
    "172.30.",
    "172.31.",
    "169.254.",
    "0.",
];

const BLOCKED_HOST_EXACT: &[&str] = &[
    "0.0.0.0",
    "metadata.google.internal",
    "metadata.tencentyun.com",
];

/// 验证 endpoint 安全性，返回 (scheme, safe_endpoint)
pub fn validate_endpoint(endpoint: &str) -> Result<(String, String), String> {
    let trimmed = endpoint.trim();

    let (scheme, host) = parse_scheme_host(trimmed)
        .ok_or_else(|| format!("Invalid endpoint URL: '{}'", trimmed))?;

    let host_lower = host.to_lowercase();
    let is_localhost = host_lower == "localhost"
        || host_lower == "127.0.0.1"
        || host_lower == "::1"
        || host_lower == "[::1]";

    if scheme != "https" && !is_localhost {
        return Err(
            "HTTP is not allowed for remote endpoints. Use HTTPS or localhost for development."
                .into(),
        );
    }

    for exact in BLOCKED_HOST_EXACT {
        if host_lower == *exact {
            return Err(format!(
                "Endpoint '{}' is blocked for security reasons.",
                host
            ));
        }
    }

    if host_lower.starts_with("fc") || host_lower.starts_with("fd") || host_lower.starts_with("fe80") {
        return Err(format!(
            "Private/internal IPv6 network endpoints are not allowed: '{}'.",
            host
        ));
    }

    if host_lower.starts_with("::ffff:") || host_lower.starts_with("[::ffff:") {
        return Err(format!(
            "IPv4-mapped IPv6 addresses are not allowed: '{}'.",
            host
        ));
    }

    for prefix in BLOCKED_HOST_PREFIXES {
        if host_lower.starts_with(prefix) {
            return Err(format!(
                "Private/internal network endpoints are not allowed: '{}'.",
                host
            ));
        }
    }

    if host.contains('@') {
        return Err("Endpoint URL must not contain credentials.".into());
    }

    Ok((scheme.to_string(), trimmed.to_string()))
}

pub fn validate_ai_request(endpoint: &str, model: &str, api_key: &str) -> Result<String, String> {
    let (_scheme, safe_endpoint) = validate_endpoint(endpoint)?;
    if model.trim().is_empty() {
        return Err("模型名称不能为空".into());
    }
    if api_key.trim().is_empty() {
        return Err("API Key 不能为空".into());
    }
    Ok(safe_endpoint)
}

/// ─── 消息安全处理 ──────────────────────────────────────
pub fn truncate_message(content: &str) -> String {
    if content.len() <= MAX_MESSAGE_CONTENT_LEN {
        content.to_string()
    } else {
        let mut truncated: String = content.chars().take(MAX_MESSAGE_CONTENT_LEN).collect();
        truncated.push_str("\n\n[消息过长，已截断]");
        truncated
    }
}

/// LLM 协议层消息（原 ChatMessage，OpenAI 兼容格式）。
///
/// 改名自 `ChatMessage`，解耦 UI 语义。扩展 agent / translate 均使用此类型。
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
    pub kind: String, // 恒为 "function"
    pub function: LlmToolCallFunction,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LlmToolCallFunction {
    pub name: String,
    pub arguments: String, // JSON 字符串（与 OpenAI 一致）
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

/// ─── 流式结果 ──────────────────────────────────────────
/// 本轮流式的最终结局（无工具调用时 `ToolCalls` 为空）。
#[derive(Debug)]
pub struct StreamOutcome {
    /// finish_reason（"stop"/"tool_calls"/"length"/...）。
    /// 当前未读，保留以便未来根据截断/拒绝原因做不同处理。
    #[allow(dead_code)]
    pub finish_reason: String,
    /// 累积的完整文本（content 增量之和）
    pub full_text: String,
    /// 完整的 tool_calls（仅 finish_reason == "tool_calls" 时有值）
    pub tool_calls: Vec<FinalizedToolCall>,
}

/// ─── SSE 流式请求配置 ──────────────────────────────────
pub struct StreamConfig<'a> {
    pub app: &'a tauri::AppHandle,
    pub endpoint: &'a str,
    pub api_key: &'a str,
    pub model: &'a str,
    pub messages: Vec<LlmMessage>,
    /// OpenAI tools 数组（None = 不启用 function calling）
    pub tools: Option<&'a [serde_json::Value]>,
    /// tool_choice（"auto"/"required"/"none"/None）
    pub tool_choice: Option<&'a str>,
    /// 文本增量回调（agent loop 用，None 时跳过）
    pub on_text_delta: Option<&'a mut (dyn FnMut(&str) + Send)>,
    /// tool_calls 增量回调（每帧 tool_calls delta 触发）
    pub on_tool_calls_delta: Option<&'a mut (dyn FnMut(&ChoiceDelta) + Send)>,
    /// 旧式事件通道：chunk_event + done_event（空字符串表示不 emit）
    pub chunk_event: &'a str,
    pub done_event: &'a str,
    pub request_id: &'a str,
    pub abort_flag: Option<&'a std::sync::atomic::AtomicBool>,
}

/// ─── 通用 SSE 流式请求 ──────────────────────────────────
/// 发起 OpenAI 兼容的流式请求。
///
/// 行为：
/// - 文本增量：emit chunk_event（如果非空）+ 调 on_text_delta（如果提供）
/// - 工具增量：调 on_tool_calls_delta（如果提供）
/// - tool_calls finalize：在 finish_reason 出现时累积完毕，返回在 StreamOutcome
/// - 结束：emit done_event（如果非空）
/// - abort：abort_flag 为 true 时立即停止并 emit done_event
pub async fn stream_openai_request(
    config: StreamConfig<'_>,
) -> Result<StreamOutcome, String> {
    let url = format!(
        "{}/chat/completions",
        config.endpoint.trim_end_matches('/')
    );

    let messages_json: Vec<serde_json::Value> = config
        .messages
        .iter()
        .map(|m| {
            let mut obj = serde_json::json!({ "role": m.role });
            if let Some(c) = &m.content {
                obj["content"] = serde_json::Value::String(truncate_message(c));
            }
            if let Some(tc) = &m.tool_calls {
                obj["tool_calls"] = serde_json::to_value(tc).unwrap_or(serde_json::Value::Null);
            }
            if let Some(id) = &m.tool_call_id {
                obj["tool_call_id"] = serde_json::Value::String(id.clone());
            }
            obj
        })
        .collect();

    let mut body = serde_json::json!({
        "model": config.model.trim(),
        "messages": messages_json,
        "stream": true
    });
    if let Some(tools) = config.tools {
        body["tools"] = serde_json::Value::Array(tools.to_vec());
    }
    if let Some(choice) = config.tool_choice {
        body["tool_choice"] = serde_json::Value::String(choice.to_string());
    }

    let response = crate::http::client()
        .post(&url)
        .header("Authorization", format!("Bearer {}", config.api_key.trim()))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            log::error!("Stream request network error: {}", e);
            "Failed to connect to API. Check your endpoint and network.".to_string()
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        log::error!("API HTTP {}: {}", status, body_text);

        return Err(match status.as_u16() {
            401 => "Authentication failed. Please check your API key.".to_string(),
            403 => "Access denied. Your API key may not have permission.".to_string(),
            429 => "Rate limited. Please wait and try again.".to_string(),
            500.. => "API server error. Please try again later.".to_string(),
            _ => format!("API returned HTTP {}", status),
        });
    }

    // ── SSE 流式解析 ──────────────────────────────────────
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut full_text = String::new();
    let mut tool_acc = ToolCallAccumulator::default();
    let mut finish_reason = String::new();

    // 把可变回调拆出来（避免借用冲突），其余字段照常通过 config 引用
    let mut on_text = config.on_text_delta;
    let mut on_tool = config.on_tool_calls_delta;

    while let Some(item) = stream.next().await {
        if let Some(flag) = config.abort_flag {
            if flag.swap(false, Ordering::SeqCst) {
                emit_done(config.app, config.done_event, config.request_id);
                return Ok(StreamOutcome {
                    finish_reason: "aborted".into(),
                    full_text,
                    tool_calls: Vec::new(),
                });
            }
        }
        let chunk = item.map_err(|e| {
            log::error!("Stream read error: {}", e);
            "Stream connection interrupted.".to_string()
        })?;
        let text = String::from_utf8_lossy(&chunk);

        // 缓冲区安全上限
        if buffer.len() + text.len() > MAX_SSE_BUFFER {
            log::error!(
                "SSE buffer exceeded {} bytes, dropping connection.",
                MAX_SSE_BUFFER
            );
            emit_done(config.app, config.done_event, config.request_id);
            return Ok(StreamOutcome {
                finish_reason: "buffer_overflow".into(),
                full_text,
                tool_calls: Vec::new(),
            });
        }

        buffer.push_str(&text);
        buffer = buffer.replace("\r\n", "\n").replace('\r', "\n");

        while let Some(event_end) = buffer.find("\n\n") {
            let event_data = buffer[..event_end].to_string();
            buffer = buffer[event_end + 2..].to_string();

            let mut data_content = String::new();
            for line in event_data.lines() {
                if let Some(rest) = line.strip_prefix("data: ") {
                    if !data_content.is_empty() {
                        data_content.push('\n');
                    }
                    data_content.push_str(rest);
                }
            }

            if data_content == "[DONE]" {
                emit_done(config.app, config.done_event, config.request_id);
                return Ok(finalize_stream(finish_reason, full_text, tool_acc));
            }

            if data_content.is_empty() {
                continue;
            }

            // 整体解析 chunk（取 finish_reason）
            #[derive(Deserialize)]
            struct Chunk {
                #[serde(default)]
                choices: Vec<ChunkChoice>,
            }
            #[derive(Deserialize)]
            struct ChunkChoice {
                #[serde(default)]
                delta: ChoiceDelta,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                finish_reason: Option<String>,
            }

            let Ok(chunk) = serde_json::from_str::<Chunk>(&data_content) else {
                continue;
            };
            let Some(choice) = chunk.choices.into_iter().next() else {
                continue;
            };

            if let Some(fr) = choice.finish_reason {
                finish_reason = fr;
            }

            let delta = choice.delta;

            // 文本增量
            if let Some(text) = &delta.content {
                if !text.is_empty() {
                    full_text.push_str(text);
                    if !config.chunk_event.is_empty() {
                        let _ = config.app.emit(
                            config.chunk_event,
                            serde_json::json!({
                                "requestId": config.request_id,
                                "content": text
                            }),
                        );
                    }
                    if let Some(cb) = on_text.as_deref_mut() {
                        cb(text);
                    }
                }
            }

            // 工具增量
            if !delta.tool_calls.is_empty() {
                tool_acc.process_delta(&delta);
                if let Some(cb) = on_tool.as_deref_mut() {
                    cb(&delta);
                }
            }
        }
    }

    emit_done(config.app, config.done_event, config.request_id);
    Ok(finalize_stream(finish_reason, full_text, tool_acc))
}

fn finalize_stream(
    finish_reason: String,
    full_text: String,
    acc: ToolCallAccumulator,
) -> StreamOutcome {
    // 仅 tool_calls 结局时尝试 finalize（length 截断时 arguments 可能不完整，跳过）
    let tool_calls = if finish_reason == "tool_calls" {
        match acc.finalize() {
            Ok(calls) => calls,
            Err(e) => {
                log::warn!("tool_calls finalize failed ({}), falling back to lenient", e);
                acc.finalize_lenient()
            }
        }
    } else {
        Vec::new()
    };
    StreamOutcome { finish_reason, full_text, tool_calls }
}

fn emit_done(app: &tauri::AppHandle, done_event: &str, request_id: &str) {
    if done_event.is_empty() {
        return;
    }
    let _ = app.emit(
        done_event,
        serde_json::json!({ "requestId": request_id }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_endpoint_rejects_private_network() {
        assert!(validate_endpoint("https://192.168.1.1/v1").is_err());
        assert!(validate_endpoint("https://10.0.0.1/v1").is_err());
        assert!(validate_endpoint("https://metadata.google.internal/v1").is_err());
    }

    #[test]
    fn validate_endpoint_accepts_https() {
        assert!(validate_endpoint("https://api.openai.com/v1").is_ok());
    }

    #[test]
    fn validate_endpoint_rejects_remote_http() {
        assert!(validate_endpoint("http://api.openai.com/v1").is_err());
    }

    #[test]
    fn validate_endpoint_accepts_localhost_http() {
        assert!(validate_endpoint("http://localhost:8080/v1").is_ok());
    }

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
}
