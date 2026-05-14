use crate::sse::{self, ChatMessage};

/// ─── 主入口 ──────────────────────────────────────────────

#[tauri::command]
pub async fn chat_stream(
    app: tauri::AppHandle,
    messages: Vec<ChatMessage>,
    endpoint: String,
    api_key: String,
    model: String,
    request_id: Option<String>,
) -> Result<(), String> {
    // ── 入口安全校验 ──────────────────────────────────────
    let (_scheme, safe_endpoint) = sse::validate_endpoint(&endpoint)?;

    if model.trim().is_empty() {
        return Err("Model name must not be empty.".into());
    }

    if api_key.trim().is_empty() {
        return Err("API key must not be empty.".into());
    }

    // 消息裁剪与截断
    let trimmed_messages = sse::trim_conversation(&messages);

    let req_id = request_id.unwrap_or_else(|| "default".to_string());

    sse::stream_openai_request(sse::StreamConfig {
        app: &app,
        endpoint: &safe_endpoint,
        api_key: &api_key,
        model: &model,
        messages: trimmed_messages,
        chunk_event: "chat-chunk",
        done_event: "chat-done",
        request_id: &req_id,
    })
    .await
}
