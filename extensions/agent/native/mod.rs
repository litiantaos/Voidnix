use crate::extensions::agent::engine::approval::{ApprovalManager, Decision};
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

/// 命令注册（局部 invoke_handler，§2.8）。
pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new("agent").build()
}

// ──────────────────────────────────────────────────────────────
// Agent 路径：agent_run / agent_approve / agent_abort
// ──────────────────────────────────────────────────────────────

/// Agent 配置（前端 invoke 时随调用传入）。安全项由 Rust 端 clamp/并集兜底（§3.4）。
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunConfig {
    /// 搜索提供商（duckduckgo / tavily）
    pub search_provider: SearchProviderConfig,
    /// 用户自定义白名单命令（追加到默认白名单，免审批）。
    pub trusted_commands: Vec<String>,
    /// 用户自定义硬禁命令（与 FORBIDDEN_FLOOR 取并集，用户只能加严）。
    #[serde(default)]
    pub forbidden_commands: Vec<String>,
    /// 用户自定义危险参数前缀（与 DENIED_ARG_FLOOR 取并集）。
    #[serde(default)]
    pub blocked_args: Vec<String>,
    /// 用户自定义 system prompt（追加到默认 harness 之后）。
    pub system_prompt: Option<String>,
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

/// 默认 system prompt（agent 扩展自管，非框架硬编码）。
const DEFAULT_SYSTEM_PROMPT: &str = r#"你是 Voidnix 内置的 AI Agent，运行在用户的 macOS 上。你的职责是帮助用户完成日常任务：回答问题、查找信息、操作文件、执行命令。

# 工具使用规则

你有两个工具可用：
- `web_search(query)`: 联网搜索。当用户问事实性/时事性问题、或需要外部知识时使用。不要对能从上下文推断答案的问题使用。
- `run_command(cmd, args)`: 在用户 macOS 上执行 shell 命令（不经过 shell，参数数组传递）。可用于浏览文件、查询系统信息、编辑文件、git 操作等。

工具调用原则：
- 简单问题直接回答，不要为了"用工具"而用工具
- 复杂任务可以连续多次调用工具（每次拿到结果后判断是否需要下一步）
- 工具结果可能被净化（secret 替换为 [REDACTED]），这是正常的安全防护

# 安全约束

- 不要尝试执行破坏性操作（如 `rm -rf /`、覆盖系统文件），这些会被硬拦
- 不要读取或外泄用户敏感数据（API key、SSH key、密码等），输出会被自动打码
- 危险命令需要用户审批，被拒后换方案而不是反复尝试

# 输出风格

- 简洁直接，避免冗长铺垫
- 代码/命令用 markdown 代码块包裹
- 中文为主（除非用户用英文提问）"#;

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
#[allow(clippy::too_many_arguments)] // Tauri 命令 IPC 契约决定参数数（state 注入 + 前端参数）
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

    // 安全底线 clamp/并集（§3.4）：agent_run 入口集中处理，不信任前端传值
    let exec_policy = policy::ExecPolicy::resolve(
        config.trusted_commands.clone(),
        config.forbidden_commands.clone(),
        config.blocked_args.clone(),
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
    // 共享全局 ApprovalManager（agent_approve 通过它路由）
    let approval_clone = approval.inner().clone();

    let input = LoopInput {
        app: app.clone(),
        endpoint: safe_endpoint,
        api_key,
        model,
        messages,
        default_system_prompt: DEFAULT_SYSTEM_PROMPT.to_string(),
        system_prompt: config.system_prompt,
        max_turns,
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
    let decision = Decision {
        approved,
        always_approve,
    };
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
pub struct AgentExtension;

#[async_trait::async_trait]
impl Extension for AgentExtension {
    fn id(&self) -> &'static str {
        "agent"
    }

    async fn setup(&self, app: &tauri::AppHandle) -> tauri::Result<()> {
        use tauri::Manager;
        // 扩展级共享 State（agent_run / agent_approve / agent_abort 命令消费）
        app.manage(engine::cancellation::SessionRegistry::default());
        app.manage(engine::approval::ApprovalManager::default());
        Ok(())
    }
}
