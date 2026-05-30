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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

pub fn trim_conversation(messages: &[ChatMessage]) -> Vec<ChatMessage> {
    if messages.len() <= MAX_CONVERSATION_MESSAGES {
        messages.to_vec()
    } else {
        let system_msg = messages.iter().find(|m| m.role == "system").cloned();
        let skip = messages.len() - (MAX_CONVERSATION_MESSAGES - 1);
        let mut trimmed: Vec<ChatMessage> = messages.iter().skip(skip).cloned().collect();
        if let Some(sys) = system_msg {
            if trimmed.first().map(|m| m.role.as_str()) != Some("system") {
                trimmed.insert(0, sys);
            }
        }
        trimmed
    }
}

/// ─── SSE 流式请求配置 ──────────────────────────────────
pub struct StreamConfig<'a> {
    pub app: &'a tauri::AppHandle,
    pub endpoint: &'a str,
    pub api_key: &'a str,
    pub model: &'a str,
    pub messages: Vec<ChatMessage>,
    pub chunk_event: &'a str,
    pub done_event: &'a str,
    pub request_id: &'a str,
    pub abort_flag: Option<&'a std::sync::atomic::AtomicBool>,
}

/// ─── 通用 SSE 流式请求 ──────────────────────────────────
/// 发起 OpenAI 兼容的流式请求，通过事件推送 chunk
pub async fn stream_openai_request(config: StreamConfig<'_>) -> Result<(), String> {
    let url = format!(
        "{}/chat/completions",
        config.endpoint.trim_end_matches('/')
    );

    let messages_json: Vec<serde_json::Value> = config
        .messages
        .iter()
        .map(|m| {
            serde_json::json!({
                "role": m.role,
                "content": truncate_message(&m.content)
            })
        })
        .collect();

    let body = serde_json::json!({
        "model": config.model.trim(),
        "messages": messages_json,
        "stream": true
    });

    let response = crate::infra::http::client()
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

    while let Some(item) = stream.next().await {
        if let Some(flag) = config.abort_flag {
            if flag.swap(false, Ordering::SeqCst) {
                let _ = config.app.emit(
                    config.done_event,
                    serde_json::json!({ "requestId": config.request_id }),
                );
                return Ok(());
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
            let _ = config.app.emit(
                config.done_event,
                serde_json::json!({ "requestId": config.request_id }),
            );
            return Ok(());
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
                let _ = config.app.emit(
                    config.done_event,
                    serde_json::json!({ "requestId": config.request_id }),
                );
                return Ok(());
            }

            if !data_content.is_empty() {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&data_content) {
                    if let Some(choices) = json.get("choices").and_then(|c| c.as_array()) {
                        if let Some(choice) = choices.first() {
                            if let Some(delta) = choice.get("delta") {
                                if let Some(content) =
                                    delta.get("content").and_then(|c| c.as_str())
                                {
                                    if !content.is_empty() {
                                        let _ = config.app.emit(
                                            config.chunk_event,
                                            serde_json::json!({
                                                "requestId": config.request_id,
                                                "content": content
                                            }),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let _ = config.app.emit(
        config.done_event,
        serde_json::json!({ "requestId": config.request_id }),
    );
    Ok(())
}
