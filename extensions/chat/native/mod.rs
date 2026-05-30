use crate::infra::sse::{self, ChatMessage};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};

static ABORT_FLAG: LazyLock<AtomicBool> = LazyLock::new(|| AtomicBool::new(false));

#[tauri::command]
pub fn chat_abort() {
    ABORT_FLAG.store(true, Ordering::SeqCst);
}

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
    let safe_endpoint = sse::validate_ai_request(&endpoint, &model, &api_key)?;

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
        abort_flag: Some(&ABORT_FLAG),
    })
    .await
}


pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new("chat").build()
}
