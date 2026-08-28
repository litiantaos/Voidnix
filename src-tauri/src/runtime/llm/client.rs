use crate::runtime::llm::parser::{ChoiceDelta, FinalizedToolCall, ToolCallAccumulator};
use crate::runtime::llm::types::LlmMessage;
use futures_util::StreamExt;
use serde::Deserialize;
use std::sync::atomic::Ordering;
use tauri::Emitter;

// ── 请求管道常量（agent + translate 共享）────
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
            log::error!("Request network error: {e}");
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
        .map_err(|e| format!("JSON parse error: {e}"))?;

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

/// 单条消息内容截断（请求管道）
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

/// SSE 流式请求配置。
/// `R` 泛型仅为测试注入 mock runtime（`tauri::test`），默认 Wry，调用方零改动。
pub struct StreamConfig<'a, R: tauri::Runtime = tauri::Wry> {
    pub app: &'a tauri::AppHandle<R>,
    pub endpoint: &'a str,
    pub api_key: &'a str,
    pub model: &'a str,
    pub messages: &'a [LlmMessage],
    pub tools: Option<&'a [serde_json::Value]>,
    pub tool_choice: Option<&'a str>,
    pub on_text_delta: Option<&'a mut (dyn FnMut(&str) + Send)>,
    /// 思考模式增量回调（reasoning_content）
    pub on_reasoning_delta: Option<&'a mut (dyn FnMut(&str) + Send)>,
    pub on_tool_calls_delta: Option<&'a mut (dyn FnMut(&ChoiceDelta) + Send)>,
    pub chunk_event: &'a str,
    pub done_event: &'a str,
    pub request_id: &'a str,
    pub abort_flag: Option<&'a std::sync::atomic::AtomicBool>,
}

/// 发起 OpenAI 兼容的流式请求。
pub async fn stream_openai_request<R: tauri::Runtime>(
    config: StreamConfig<'_, R>,
) -> Result<StreamOutcome, String> {
    let url = format!("{}/chat/completions", config.endpoint.trim_end_matches('/'));

    let messages_json = messages_to_json(config.messages);

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

    // LLM 流式总时长不可控（长推理 + 工具轮次 + 长输出远超 120s），走无整体超时的
    // 流式 client；读间隙超时兜底 stalled 连接
    let response = crate::http::stream_client()
        .post(&url)
        .header("Authorization", format!("Bearer {}", config.api_key.trim()))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            log::error!("Stream request network error: {e}");
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
    let mut on_reasoning = config.on_reasoning_delta;
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
            log::error!("Stream read error: {e}");
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

        // 大部分 SSE 服务器不发 \r，跳过两次 String 分配
        if text.contains('\r') {
            buffer.push_str(&text.replace("\r\n", "\n").replace('\r', "\n"));
        } else {
            buffer.push_str(&text);
        }

        while let Some(event_end) = buffer.find("\n\n") {
            let event_data = buffer[..event_end].to_string();
            buffer.drain(..event_end + 2);

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
                // 裸 JSON 行（无 data: 前缀）也可能是服务端错误负载（GLM 1301 内容审查等）
                if let Some(msg) = extract_stream_error(&event_data) {
                    log::error!("LLM stream error payload: {msg}");
                    return Err(msg);
                }
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
                // choices 为空的事件可能是 `{"error":{...}}` 负载（Chunk 宽松反序列化
                // 下 choices 默认空，错误负载会被当 keepalive 静默吞掉）——提取后上抛
                if let Some(msg) = extract_stream_error(&data_content) {
                    log::error!("LLM stream error payload: {msg}");
                    return Err(msg);
                }
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

            if let Some(text) = &delta.reasoning_content {
                if !text.is_empty() {
                    if let Some(cb) = on_reasoning.as_deref_mut() {
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

    // 流在此自然结束（EOF）= 服务端未发 [DONE]（[DONE] 分支已提前 return）。
    // 先查残留 buffer：服务端中断常以错误负载收尾（GLM 内容审查 1301 等以裸 JSON 行
    // 下发，无 data: 前缀也无终止空行，构不成完整事件）——提取出真实原因上抛，
    // 而非笼统的 premature。若连 finish_reason 也没收到，才是无信号的提前断流——
    // 曾按正常完成静默收尾，截断的 partial 文本以 Completed 终结（表现为「输出莫名
    // 其妙中断」）。必须显式 Err：前端保留已流出的 partial 文本并以 error notice 收尾
    if let Some(msg) = extract_stream_error(&buffer) {
        log::error!("LLM stream error payload: {msg}");
        return Err(msg);
    }
    if finish_reason.is_empty() {
        log::error!("SSE stream ended without [DONE] or finish_reason, treating as truncated");
        return Err("Stream ended prematurely. The reply may be incomplete.".to_string());
    }
    emit_done(config.app, config.done_event, config.request_id);
    Ok(finalize_stream(finish_reason, full_text, tool_acc))
}

/// 从 SSE 原始文本提取服务端错误负载（`{"error":{"code","message"}}`）。
/// 兼容三种形态：完整事件的 data 体、无 data: 前缀的裸 JSON 行、EOF 残留 buffer。
/// 逐候选（整体 / 倒序逐行）尝试解析，命中返回 `[code] message`，无错误负载返回 None。
fn extract_stream_error(raw: &str) -> Option<String> {
    let candidates = std::iter::once(raw.trim()).chain(
        raw.lines()
            .rev()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty()),
    );
    for candidate in candidates {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(candidate) else {
            continue;
        };
        let Some(err) = v.get("error") else {
            continue;
        };
        let msg = err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("LLM stream error");
        return Some(match err.get("code").and_then(|c| c.as_str()) {
            Some(code) => format!("[{code}] {msg}"),
            None => msg.to_string(),
        });
    }
    None
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
                    "tool_calls finalize failed ({}), args_preview=[{}], falling back to lenient",
                    e,
                    acc.args_preview(120)
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

fn emit_done<R: tauri::Runtime>(app: &tauri::AppHandle<R>, done_event: &str, request_id: &str) {
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

    // ── SSE 断流回归测试（本地 mock server 回放事件后关连接）──────────────────

    /// 起一个一次性 SSE mock：接受单连接、丢弃请求、原样回放 events、关闭连接。
    /// 返回 endpoint（http://127.0.0.1:port/v1）。
    async fn spawn_sse_server(events: &str) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let body = events.to_string();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 8192];
            let _ = sock.read(&mut buf).await;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n{body}"
            );
            let _ = sock.write_all(resp.as_bytes()).await;
            let _ = sock.shutdown().await;
        });
        format!("http://{addr}/v1")
    }

    async fn run_stream(
        endpoint: &str,
        on_text: &mut (dyn FnMut(&str) + Send),
    ) -> Result<StreamOutcome, String> {
        let app = tauri::test::mock_app();
        let handle = app.handle().clone();
        stream_openai_request(StreamConfig {
            app: &handle,
            endpoint,
            api_key: "k",
            model: "m",
            messages: &[LlmMessage::user("hi")],
            tools: None,
            tool_choice: None,
            on_text_delta: Some(on_text),
            on_reasoning_delta: None,
            on_tool_calls_delta: None,
            chunk_event: "",
            done_event: "",
            request_id: "",
            abort_flag: None,
        })
        .await
    }

    #[tokio::test]
    async fn stream_eof_without_done_or_finish_is_error() {
        // 服务端/代理中途断流：只有 content 增量，无 [DONE] 无 finish_reason
        let url =
            spawn_sse_server("data: {\"choices\":[{\"delta\":{\"content\":\"你好\"}}]}\n\n").await;
        let mut got = String::new();
        let res = run_stream(&url, &mut |d: &str| got.push_str(d)).await;
        assert!(res.is_err(), "premature EOF must be Err, got Ok: {res:?}");
        // partial 文本仍须经回调流出（前端气泡保留已生成内容）
        assert_eq!(got, "你好");
    }

    #[tokio::test]
    async fn stream_with_done_is_ok() {
        let url = spawn_sse_server(concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"你好\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n",
        ))
        .await;
        let mut got = String::new();
        let outcome = run_stream(&url, &mut |d: &str| got.push_str(d))
            .await
            .unwrap();
        assert_eq!(outcome.full_text, "你好");
        assert_eq!(got, "你好");
    }

    #[tokio::test]
    async fn stream_eof_with_finish_reason_is_ok() {
        // finish_reason 已收到即生成完成，[DONE] 丢失不影响正确性
        let url = spawn_sse_server(concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"你好\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
        ))
        .await;
        let mut got = String::new();
        let outcome = run_stream(&url, &mut |d: &str| got.push_str(d))
            .await
            .unwrap();
        assert_eq!(outcome.full_text, "你好");
        assert_eq!(got, "你好");
    }

    #[tokio::test]
    async fn stream_error_event_yields_server_message() {
        // 服务端错误负载走完整事件（data: 前缀 + 终止空行）——真实原因上抛，partial 保留
        let url = spawn_sse_server(concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"部分\"}}]}\n\n",
            "data: {\"error\":{\"code\":\"1301\",\"message\":\"系统检测到敏感内容\"}}\n\n",
        ))
        .await;
        let mut got = String::new();
        let res = run_stream(&url, &mut |d: &str| got.push_str(d)).await;
        let err = res.err().expect("error payload must surface");
        assert!(
            err.contains('[') && err.contains("1301") && err.contains("敏感内容"),
            "{err}"
        );
        assert_eq!(got, "部分");
    }

    #[tokio::test]
    async fn stream_bare_error_line_at_eof_yields_server_message() {
        // GLM 形态：错误以裸 JSON 行下发（无 data: 前缀、无终止空行），残留在 EOF buffer
        let url = spawn_sse_server(concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"部分\"}}]}\n\n",
            "{\"error\":{\"code\":\"1301\",\"message\":\"系统检测到敏感内容\"}}\n",
        ))
        .await;
        let mut got = String::new();
        let res = run_stream(&url, &mut |d: &str| got.push_str(d)).await;
        let err = res.err().expect("bare error line must surface");
        assert!(err.contains("1301") && err.contains("敏感内容"), "{err}");
        assert_eq!(got, "部分");
    }

    #[tokio::test]
    async fn stream_keepalive_empty_choices_is_not_error() {
        // 空 choices 事件（keepalive 等）不误判为错误
        let url = spawn_sse_server(concat!(
            "data: {\"choices\":[]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"你好\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n",
        ))
        .await;
        let mut got = String::new();
        let outcome = run_stream(&url, &mut |d: &str| got.push_str(d))
            .await
            .unwrap();
        assert_eq!(outcome.full_text, "你好");
    }
}
