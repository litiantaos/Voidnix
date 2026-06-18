use crate::extensions::agent::engine::approval::{ApprovalManager, Decision};
use crate::extensions::agent::engine::cancellation::SessionRegistry;
use crate::extensions::agent::engine::loop_runner::{run_loop, LoopInput};
use crate::extensions::agent::engine::tool_registry::ToolRegistry;
use crate::extensions::agent::engine::AgentEvent;
use crate::runtime::registry::Extension;
use crate::runtime::llm::{self, LlmMessage};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::LazyLock;
use tauri::ipc::Channel;
use tokio_util::sync::CancellationToken;

pub mod engine;
mod tools;

// ──────────────────────────────────────────────────────────────
// 旧 chat_stream 路径（保留兼容，D 阶段统一废弃）
// ──────────────────────────────────────────────────────────────

static ABORT_FLAG: LazyLock<AtomicBool> = LazyLock::new(|| AtomicBool::new(false));

#[tauri::command]
pub fn chat_abort() {
    ABORT_FLAG.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub async fn chat_stream(
    app: tauri::AppHandle,
    messages: Vec<LlmMessage>,
    endpoint: String,
    api_key: String,
    model: String,
    request_id: Option<String>,
) -> Result<(), String> {
    let safe_endpoint = llm::validate_ai_request(&endpoint, &model, &api_key)?;
    let trimmed_messages = llm::trim_conversation(&messages);
    let req_id = request_id.unwrap_or_else(|| "default".to_string());

    llm::stream_openai_request(llm::StreamConfig {
        app: &app,
        endpoint: &safe_endpoint,
        api_key: &api_key,
        model: &model,
        messages: trimmed_messages,
        tools: None,
        tool_choice: None,
        on_text_delta: None,
        on_tool_calls_delta: None,
        chunk_event: "chat-chunk",
        done_event: "chat-done",
        request_id: &req_id,
        abort_flag: Some(&ABORT_FLAG),
    })
    .await
    .map(|_| ())
}

// ──────────────────────────────────────────────────────────────
// Agent 路径：agent_run / agent_approve / agent_abort
// ──────────────────────────────────────────────────────────────

/// Agent 配置（前端 invoke 时随调用传入）。
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunConfig {
    /// 搜索提供商（duckduckgo / tavily）
    pub search_provider: SearchProviderConfig,
    /// 用户自定义白名单命令（追加到默认白名单，免审批）。
    pub trusted_commands: Vec<String>,
    /// 用户自定义 system prompt（追加到默认 harness 之后）。
    pub system_prompt: Option<String>,
}

#[derive(serde::Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SearchProviderConfig {
    /// 保留协议字段（前端固定 "tavily"，未来扩展 Brave/Serper 等）。
    /// Rust 端目前只读 api_key。
    #[allow(dead_code)]
    pub r#type: String,
    pub api_key: String,
}

/// 启动一次 agent run。
///
/// 立即返回 session_id；后续事件通过 on_event Channel 推送。
/// 用户审批通过 agent_approve；中断通过 agent_abort。
#[tauri::command]
pub async fn agent_run(
    app: tauri::AppHandle,
    sessions: tauri::State<'_, SessionRegistry>,
    approval: tauri::State<'_, ApprovalManager>,
    messages: Vec<LlmMessage>,
    endpoint: String,
    api_key: String,
    model: String,
    session_id: String,
    config: AgentRunConfig,
    on_event: Channel<AgentEvent>,
) -> Result<String, String> {
    // 安全校验
    let safe_endpoint = llm::validate_ai_request(&endpoint, &model, &api_key)?;

    // 构造本次的 ToolRegistry（工具始终启用）
    let tool_registry = {
        let reg = ToolRegistry::new()
            .register(tools::web_search::WebSearchTool::new(config.search_provider.clone()))
            .register(tools::run_command::RunCommandTool::new(config.trusted_commands));
        Arc::new(reg)
    };

    let tools_schema = tool_registry.collect_tools_schema();

    let cancel = CancellationToken::new();
    // 共享全局 ApprovalManager（agent_approve 通过它路由）
    let approval_clone = approval.inner().clone();

    let input = LoopInput {
        app: app.clone(),
        endpoint: safe_endpoint,
        api_key,
        model,
        messages,
        system_prompt: config.system_prompt,
        tools_schema,
        tool_registry,
        channel: on_event,
        cancel: cancel.clone(),
        approval: approval_clone,
    };

    sessions.register(session_id.clone(), cancel.clone());

    let handle = tauri::async_runtime::spawn(async move {
        run_loop(input).await;
    });
    sessions.set_handle(&session_id, handle);

    Ok(session_id)
}

/// 用户审批回复（通过全局 ApprovalManager 路由）。
/// approval_id 即 tool_call.id（前端 part 用的同一 id）。
#[tauri::command]
pub async fn agent_approve(
    approval: tauri::State<'_, ApprovalManager>,
    approval_id: String,
    approved: bool,
    always_approve: bool,
) -> Result<bool, String> {
    let decision = Decision { approved, always_approve };
    Ok(approval.resolve(&approval_id, decision))
}

/// 中断 agent run。
#[tauri::command]
pub async fn agent_abort(
    sessions: tauri::State<'_, SessionRegistry>,
    session_id: String,
) -> Result<bool, String> {
    Ok(sessions.cancel(&session_id))
}

/// Agent 扩展。
pub struct Plugin;

impl Extension for Plugin {
    fn id(&self) -> &'static str {
        "agent"
    }
}
