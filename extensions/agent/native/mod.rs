use crate::extensions::agent::engine::cancellation::SessionRegistry;
use crate::extensions::agent::engine::loop_runner::{run_loop, LoopInput};
use crate::extensions::agent::engine::tool_registry::ToolRegistry;
use crate::extensions::agent::engine::AgentEvent;
use crate::runtime::llm::{self, LlmMessage};
use crate::runtime::registry::Extension;
use std::sync::Arc;
use tauri::ipc::Channel;
use tokio_util::sync::CancellationToken;

pub mod engine;
mod policy;
mod tools;

// ──────────────────────────────────────────────────────────────
// Agent 路径：agent_run / agent_abort
// ──────────────────────────────────────────────────────────────

/// Agent 配置（前端 invoke 时随调用传入）。资源上限由 Rust 端 clamp 兜底。
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunConfig {
    /// 搜索提供商（前端固定 tavily，保留 type 字段便于扩展）
    pub search_provider: SearchProviderConfig,
    /// system prompt（扩展自管，前端 config 直传，空串则不注入 system 消息）。
    #[serde(default)]
    pub system_prompt: String,
    /// 单次对话最大轮次（None = 用默认值；Rust 端 clamp [1,50]）。
    #[serde(default)]
    pub max_turns: Option<usize>,
    /// 资源上限（None = 用默认值；Rust 端 clamp 到 policy floor/cap）。
    #[serde(default)]
    pub max_cpu_seconds: Option<u64>,
    #[serde(default)]
    pub max_memory_mb: Option<u64>,
    #[serde(default)]
    pub max_open_files: Option<u64>,
    #[serde(default)]
    pub execution_timeout: Option<u64>,
    #[serde(default)]
    pub max_output_bytes: Option<usize>,
}

/// 默认 system prompt 由前端 config 自管（defineConfig 默认值），Rust 端只消费。

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
/// 中断通过 agent_abort。
#[tauri::command]
#[allow(clippy::too_many_arguments)] // Tauri 命令 IPC 契约决定参数数（state 注入 + 前端参数）
pub async fn agent_run(
    app: tauri::AppHandle,
    sessions: tauri::State<'_, SessionRegistry>,
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

    // 资源上限 clamp（agent_run 入口集中处理，不信任前端传值）
    let exec_policy = policy::ExecPolicy::resolve(
        config
            .max_cpu_seconds
            .unwrap_or(policy::DEFAULT_MAX_CPU_SECS),
        config
            .max_memory_mb
            .unwrap_or(policy::DEFAULT_MAX_MEMORY_MB),
        config
            .max_open_files
            .unwrap_or(policy::DEFAULT_MAX_OPEN_FILES),
        config
            .execution_timeout
            .unwrap_or(policy::DEFAULT_EXECUTION_TIMEOUT_SECS),
        config
            .max_output_bytes
            .unwrap_or(policy::DEFAULT_MAX_OUTPUT_BYTES),
    );
    let max_turns = config
        .max_turns
        .unwrap_or(policy::DEFAULT_MAX_TURNS)
        .clamp(policy::MAX_TURNS.0, policy::MAX_TURNS.1);

    // 构造本次的 ToolRegistry（工具始终启用）
    let tool_registry = {
        let reg = ToolRegistry::new()
            .register(tools::web_search::WebSearchTool::new(
                config.search_provider.clone(),
            ))
            .register(tools::run_command::RunCommandTool::new(exec_policy));
        Arc::new(reg)
    };

    let tools_schema = tool_registry.collect_tools_schema();

    let cancel = CancellationToken::new();

    let input = LoopInput {
        app: app.clone(),
        endpoint: safe_endpoint,
        api_key,
        model,
        messages,
        system_prompt: config.system_prompt,
        max_turns,
        tools_schema,
        tool_registry,
        channel: on_event,
        cancel: cancel.clone(),
    };

    // 先 register token，使 abort 在 spawn 后立刻可 cancel；再 spawn；set_handle 若发现
    // 已被 cancel 移除则 abort JoinHandle，关闭 register→set_handle 竞态窗口。
    sessions.register(session_id.clone(), cancel.clone());

    // SessionRegistry 是 cheap clone（Arc）；task 结束后 unregister，避免只增不减
    let sessions_for_cleanup = sessions.inner().clone();
    let sid_for_cleanup = session_id.clone();
    let handle = tauri::async_runtime::spawn(async move {
        run_loop(input).await;
        sessions_for_cleanup.unregister(&sid_for_cleanup);
    });
    let _ = sessions.set_handle(&session_id, handle);

    Ok(session_id)
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
pub struct AgentExtension;

#[async_trait::async_trait]
impl Extension for AgentExtension {
    fn id(&self) -> &'static str {
        "agent"
    }

    async fn setup(&self, app: &tauri::AppHandle) -> tauri::Result<()> {
        use tauri::Manager;
        // 扩展级共享 State（agent_run / agent_abort 命令消费）
        let sessions = SessionRegistry::default();
        app.manage(sessions.clone());
        // run 进行中否决 WebContent 内存重载：navigate 即断 Channel，流式事件永久
        // 丢失、run 沦为孤儿（只能被 boot 恢复逻辑 abort）——隐藏窗口不打断输出
        crate::runtime::window::register_reload_guard(Arc::new(move || sessions.has_active()));
        Ok(())
    }
}
