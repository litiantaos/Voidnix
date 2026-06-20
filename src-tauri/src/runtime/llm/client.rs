use crate::runtime::llm::parser::{ChoiceDelta, FinalizedToolCall, ToolCallAccumulator};
use crate::runtime::llm::types::LlmMessage;
use futures_util::StreamExt;
use serde::Deserialize;
use std::sync::atomic::Ordering;
use tauri::Emitter;

// ── 请求管道常量（agent + translate 共享，§1.1 溶解自 security.rs）────
/// SSE 缓冲上限（1 MiB），防止无界 buffer 增长
const MAX_SSE_BUFFER: usize = 1_048_576;
/// 单条消息内容上限（字符数，P4-rs4 统一为字符而非字节，避免多字节中文截断边界不一致）
const MAX_MESSAGE_CONTENT_LEN: usize = 32_768;

// ── SSRF 防护：endpoint 校验复用 crate::http::validate_endpoint_url（H3 单一真相源）──

/// 校验 AI 请求 endpoint/model/api_key，返回 safe endpoint。
pub fn validate_ai_request(endpoint: &str, model: &str, api_key: &str) -> Result<String, String> {
    let (_scheme, safe_endpoint) = crate::http::validate_endpoint_url(endpoint)?;
    if model.trim().is_empty() {
        return Err("模型名称不能为空".into());
    }
    if api_key.trim().is_empty() {
        return Err("API Key 不能为空".into());
    }
    Ok(safe_endpoint)
}

/// LlmMessage → OpenAI 协议 JSON（snake_case key，agent_run 入参的 camelCase 由 types.rs rename 处理）。
/// stream_openai_request 与 openai_request_once 共享，保证请求体一致。
fn messages_to_json(messages: &[LlmMessage]) -> Vec<serde_json::Value> {
    messages
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
        .collect()
}

/// HTTP 错误状态码 → 用户友好消息（stream / non-stream 共享）。
fn map_api_error(status: reqwest::StatusCode) -> String {
    log::error!("API HTTP error: {}", status);
    match status.as_u16() {
        401 => "Authentication failed. Please check your API key.".into(),
        403 => "Access denied. Your API key may not have permission.".into(),
        429 => "Rate limited. Please wait and try again.".into(),
        500.. => "API server error. Please try again later.".into(),
        _ => format!("API returned HTTP {}", status),
    }
}

/// 非流式 OpenAI 兼容请求：translate_ai 等无需流式的消费者使用。
/// 复用 validate / messages_to_json / map_api_error 共享管道，消除双轨实现。
pub async fn openai_request_once(
    endpoint: &str,
    api_key: &str,
    model: &str,
    messages: Vec<LlmMessage>,
) -> Result<String, String> {
    let safe_endpoint = validate_ai_request(endpoint, model, api_key)?;
    let url = format!("{}/chat/completions", safe_endpoint.trim_end_matches('/'));

    let body = serde_json::json!({
        "model": model.trim(),
        "messages": messages_to_json(&messages),
        "stream": false
    });

    let response = crate::http::client()
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key.trim()))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            log::error!("Request network error: {}", e);
            "Failed to connect to API. Check your endpoint and network.".to_string()
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let _ = response.text().await;
        return Err(map_api_error(status));
    }

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("JSON parse error: {}", e))?;

    let content = json
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("");
    Ok(content.to_string())
}

/// 单条消息内容截断（请求管道，§1.1）
///
/// P4-rs4：阈值与截断都按字符数（`chars().count()`），避免旧实现字节判定 + 字符截断
/// 的边界不一致（多字节中文内容的实际字节上限可达 ~128KiB）。
fn truncate_message(content: &str) -> String {
    if content.chars().count() <= MAX_MESSAGE_CONTENT_LEN {
        return content.to_string();
    }
    let mut truncated: String = content.chars().take(MAX_MESSAGE_CONTENT_LEN).collect();
    truncated.push_str("\n\n[消息过长，已截断]");
    truncated
}

/// 本轮流式的最终结局（无工具调用时 tool_calls 为空）。
#[derive(Debug)]
pub struct StreamOutcome {
    pub full_text: String,
    pub tool_calls: Vec<FinalizedToolCall>,
}

/// SSE 流式请求配置
pub struct StreamConfig<'a> {
    pub app: &'a tauri::AppHandle,
    pub endpoint: &'a str,
    pub api_key: &'a str,
    pub model: &'a str,
    pub messages: Vec<LlmMessage>,
    pub tools: Option<&'a [serde_json::Value]>,
    pub tool_choice: Option<&'a str>,
    pub on_text_delta: Option<&'a mut (dyn FnMut(&str) + Send)>,
    pub on_tool_calls_delta: Option<&'a mut (dyn FnMut(&ChoiceDelta) + Send)>,
    pub chunk_event: &'a str,
    pub done_event: &'a str,
    pub request_id: &'a str,
    pub abort_flag: Option<&'a std::sync::atomic::AtomicBool>,
}

/// 发起 OpenAI 兼容的流式请求。
pub async fn stream_openai_request(config: StreamConfig<'_>) -> Result<StreamOutcome, String> {
    let url = format!("{}/chat/completions", config.endpoint.trim_end_matches('/'));

    let messages_json = messages_to_json(&config.messages);

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
        return Err(map_api_error(status));
    }

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut full_text = String::new();
    let mut tool_acc = ToolCallAccumulator::default();
    let mut finish_reason = String::new();

    let mut on_text = config.on_text_delta;
    let mut on_tool = config.on_tool_calls_delta;

    while let Some(item) = stream.next().await {
        if let Some(flag) = config.abort_flag {
            if flag.swap(false, Ordering::SeqCst) {
                emit_done(config.app, config.done_event, config.request_id);
                return Ok(StreamOutcome {
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

        if buffer.len() + text.len() > MAX_SSE_BUFFER {
            log::error!(
                "SSE buffer exceeded {} bytes, dropping connection.",
                MAX_SSE_BUFFER
            );
            emit_done(config.app, config.done_event, config.request_id);
            return Ok(StreamOutcome {
                full_text,
                tool_calls: Vec::new(),
            });
        }

        buffer.push_str(&text.replace("\r\n", "\n").replace('\r', "\n"));

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

            if let Some(text) = &delta.content {
                if !text.is_empty() {
                    full_text.push_str(text);
                    if !config.chunk_event.is_empty() {
                        let _ = config.app.emit(
                            config.chunk_event,
                            serde_json::json!({ "requestId": config.request_id, "content": text }),
                        );
                    }
                    if let Some(cb) = on_text.as_deref_mut() {
                        cb(text);
                    }
                }
            }

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
    let tool_calls = if finish_reason == "tool_calls" {
        match acc.finalize() {
            Ok(calls) => calls,
            Err(e) => {
                log::warn!(
                    "tool_calls finalize failed ({}), falling back to lenient",
                    e
                );
                acc.finalize_lenient()
            }
        }
    } else {
        // finish_reason 非 tool_calls 但 accumulator 有未完成的 tool_calls 分片时提示
        // （LLM 异常输出 tool_calls 却以 stop 结束的边缘情况）
        let lenient = acc.finalize_lenient();
        if !lenient.is_empty() {
            log::warn!(
                "discarded {} tool_calls with unexpected finish_reason='{}'",
                lenient.len(),
                finish_reason
            );
        }
        Vec::new()
    };
    StreamOutcome {
        full_text,
        tool_calls,
    }
}

fn emit_done(app: &tauri::AppHandle, done_event: &str, request_id: &str) {
    if done_event.is_empty() {
        return;
    }
    let _ = app.emit(done_event, serde_json::json!({ "requestId": request_id }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_ai_request_rejects_private_network() {
        // H3：SSRF 校验复用 http::validate_endpoint_url（单一真相源）
        assert!(validate_ai_request("https://192.168.1.1/v1", "gpt-4", "k").is_err());
        assert!(validate_ai_request("https://10.0.0.1/v1", "gpt-4", "k").is_err());
        assert!(validate_ai_request("https://metadata.google.internal/v1", "gpt-4", "k").is_err());
    }

    #[test]
    fn validate_ai_request_accepts_https() {
        assert!(validate_ai_request("https://api.openai.com/v1", "gpt-4", "k").is_ok());
    }

    #[test]
    fn validate_ai_request_rejects_remote_http() {
        assert!(validate_ai_request("http://api.openai.com/v1", "gpt-4", "k").is_err());
    }

    #[test]
    fn validate_ai_request_accepts_localhost_http() {
        // 开发场景：本地 LLM endpoint 允许 http
        assert!(validate_ai_request("http://localhost:8080/v1", "gpt-4", "k").is_ok());
    }

    #[test]
    fn validate_ai_request_rejects_ipv6_private() {
        // H3：IPv6 解析由 http::validate_endpoint_url 处理，覆盖 [fc00::1] 私网
        assert!(validate_ai_request("https://[fc00::1]/v1", "gpt-4", "k").is_err());
        assert!(validate_ai_request("https://[fe80::1]/v1", "gpt-4", "k").is_err());
        // loopback（[::1]）按 endpoint 策略属 localhost，允许 http 开发
        assert!(validate_ai_request("http://[::1]:8080/v1", "gpt-4", "k").is_ok());
    }

    #[test]
    fn truncate_message_respects_limit() {
        let short = "hi";
        assert_eq!(truncate_message(short), "hi");
        let long = "a".repeat(MAX_MESSAGE_CONTENT_LEN + 100);
        let t = truncate_message(&long);
        assert!(t.ends_with("[消息过长，已截断]"));
        assert!(t.len() < long.len());
    }
}
