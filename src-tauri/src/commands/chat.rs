use serde::{Deserialize, Serialize};
use tauri::Emitter;
use futures_util::StreamExt;

/// ─── 安全常量 ───────────────────────────────────────────
const MAX_SSE_BUFFER: usize = 1_048_576; // 1 MiB — 防止无界 buffer 增长
const MAX_MESSAGE_CONTENT_LEN: usize = 32_768; // 32 KiB — 单条消息上限
const MAX_CONVERSATION_MESSAGES: usize = 100; // 历史消息条数硬上限

/// ─── SSRF 防护 ──────────────────────────────────────────

/// 手动解析 URL 提取 scheme + host，不依赖 url crate
fn parse_scheme_host(raw: &str) -> Option<(&str, &str)> {
    let s = raw.trim();
    // 找到 "://" 分隔符
    let scheme_end = s.find("://")?;
    let scheme = &s[..scheme_end];
    let rest = &s[scheme_end + 3..];
    // host 截止于 '/' 或 '?' 或 '#' 或 ':'
    let host_end = rest.find(|c: char| c == '/' || c == '?' || c == '#').unwrap_or(rest.len());
    let host_port = &rest[..host_end];
    // 去掉端口号
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
    "172.16.", "172.17.", "172.18.", "172.19.",
    "172.20.", "172.21.", "172.22.", "172.23.",
    "172.24.", "172.25.", "172.26.", "172.27.",
    "172.28.", "172.29.", "172.30.", "172.31.",
    "169.254.",
    "0.",
];

fn validate_endpoint(endpoint: &str) -> Result<(String, String), String> {
    let trimmed = endpoint.trim();

    let (scheme, host) = parse_scheme_host(trimmed)
        .ok_or_else(|| format!("Invalid endpoint URL: '{}'", trimmed))?;

    // 强制 HTTPS（除非显式 localhost 用于开发）
    let host_lower = host.to_lowercase();
    let is_localhost = host_lower == "localhost"
        || host_lower == "127.0.0.1"
        || host_lower == "[::1]";

    if scheme != "https" && !is_localhost {
        return Err(
            "HTTP is not allowed for remote endpoints. Use HTTPS or localhost for development."
                .into(),
        );
    }

    // 阻止内网/危险主机名
    if host_lower == "0.0.0.0"
        || host_lower == "[::1]"
        || host_lower == "metadata.google.internal"
        || host_lower == "metadata.tencentyun.com"
    {
        return Err(format!(
            "Endpoint '{}' is blocked for security reasons.",
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

    // 拒绝 URL 中含 @ 的 userinfo 注入
    if host.contains('@') {
        return Err("Endpoint URL must not contain credentials.".into());
    }

    Ok((scheme.to_string(), trimmed.to_string()))
}

/// ─── 消息安全处理 ──────────────────────────────────────

fn truncate_message(content: &str) -> String {
    if content.len() <= MAX_MESSAGE_CONTENT_LEN {
        content.to_string()
    } else {
        let mut truncated: String = content.chars().take(MAX_MESSAGE_CONTENT_LEN).collect();
        truncated.push_str("\n\n[消息过长，已截断]");
        truncated
    }
}

fn trim_conversation(messages: &[ChatMessage]) -> Vec<ChatMessage> {
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// ─── 主入口 ──────────────────────────────────────────────

#[tauri::command]
pub async fn chat_stream(
    app: tauri::AppHandle,
    messages: Vec<ChatMessage>,
    endpoint: String,
    api_key: String,
    model: String,
) -> Result<(), String> {
    // ── 入口安全校验 ──────────────────────────────────────
    let (_scheme, safe_endpoint) = validate_endpoint(&endpoint)?;

    if model.trim().is_empty() {
        return Err("Model name must not be empty.".into());
    }

    if api_key.trim().is_empty() {
        return Err("API key must not be empty.".into());
    }

    // 消息裁剪与截断
    let trimmed_messages = trim_conversation(&messages);

    stream_openai(&app, &trimmed_messages, &safe_endpoint, &api_key, &model).await
}

/// ─── OpenAI 兼容流式请求 ──────────────────────────────────

async fn stream_openai(
    app: &tauri::AppHandle,
    messages: &[ChatMessage],
    endpoint: &str,
    api_key: &str,
    model: &str,
) -> Result<(), String> {
    let url = format!(
        "{}/chat/completions",
        endpoint.trim_end_matches('/')
    );

    let messages_json: Vec<serde_json::Value> = messages
        .iter()
        .map(|m| {
            serde_json::json!({
                "role": m.role,
                "content": truncate_message(&m.content)
            })
        })
        .collect();

    let body = serde_json::json!({
        "model": model.trim(),
        "messages": messages_json,
        "stream": true
    });

    let response = crate::http::client()
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key.trim()))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            log::error!("Chat stream network error: {}", e);
            "Failed to connect to chat API. Check your endpoint and network.".to_string()
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        log::error!("Chat API HTTP {}: {}", status, body_text);

        return Err(match status.as_u16() {
            401 => "Authentication failed. Please check your API key.".to_string(),
            403 => "Access denied. Your API key may not have permission.".to_string(),
            429 => "Rate limited. Please wait and try again.".to_string(),
            500.. => "Chat API server error. Please try again later.".to_string(),
            _ => format!("Chat API returned HTTP {}", status),
        });
    }

    // ── SSE 流式解析 ──────────────────────────────────────
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    while let Some(item) = stream.next().await {
        let chunk = item.map_err(|e| {
            log::error!("Chat stream read error: {}", e);
            "Stream connection interrupted.".to_string()
        })?;
        let text = String::from_utf8_lossy(&chunk);

        // 缓冲区安全上限
        if buffer.len() + text.len() > MAX_SSE_BUFFER {
            log::error!(
                "SSE buffer exceeded {} bytes, dropping connection.",
                MAX_SSE_BUFFER
            );
            let _ = app.emit("chat-done", serde_json::json!({}));
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
                let _ = app.emit("chat-done", serde_json::json!({}));
                return Ok(());
            }

            if !data_content.is_empty() {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&data_content) {
                    if let Some(choices) = json.get("choices").and_then(|c| c.as_array()) {
                        if let Some(choice) = choices.first() {
                            if let Some(delta) = choice.get("delta") {
                                if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                    if !content.is_empty() {
                                        let _ = app.emit("chat-chunk", serde_json::json!({
                                            "content": content
                                        }));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let _ = app.emit("chat-done", serde_json::json!({}));
    Ok(())
}

