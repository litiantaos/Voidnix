//! Agent loop runner —— 后台 task 主循环。
//!
//! 数据流：
//! ```
//! loop {
//!   stream_openai_request(messages, tools) → outcome
//!   match outcome {
//!     Text(s)     → emit Completed, break
//!     ToolCalls(c)→ for each call {
//!                     if needs_approval { emit ApprovalRequired; await oneshot }
//!                     execute tool; scrub output; emit ToolResult
//!                     append tool message to history
//!                   }
//!                   continue
//!   }
//! }
//! ```

use std::sync::Arc;

use tauri::ipc::Channel;
use tokio::select;
use tokio_util::sync::CancellationToken;

use crate::extensions::agent::engine::approval::{ApprovalManager, Decision};
use crate::extensions::agent::engine::tool_registry::ToolRegistry;
use crate::extensions::agent::engine::AgentEvent;
use crate::runtime::llm::parser::FinalizedToolCall;
use crate::runtime::llm::{self, LlmMessage, LlmToolCall, StreamConfig};

use super::secret_scrub::scrub_secret;

/// 审批超时上限（H13）：用户离开/不响应时避免 loop 永久阻塞、session 残留。
const APPROVAL_TIMEOUT_SECS: u64 = 300;

/// Agent loop 的输入配置。
pub struct LoopInput {
    pub app: tauri::AppHandle,
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
    pub messages: Vec<LlmMessage>,
    /// 默认 system prompt（由扩展传入，非框架硬编码）。
    pub default_system_prompt: String,
    /// 用户自定义 system prompt（None = 仅用默认；Some = 追加到默认之后）。
    pub system_prompt: Option<String>,
    /// 单次 agent run 的最多工具调用轮次（防失控）。
    pub max_turns: usize,
    pub tools_schema: Vec<serde_json::Value>,
    pub tool_registry: Arc<ToolRegistry>,
    pub channel: Channel<AgentEvent>,
    pub cancel: CancellationToken,
    pub approval: ApprovalManager,
}

/// 主循环。所有错误通过 `AgentEvent::Error` 推给前端后退出。
pub async fn run_loop(mut input: LoopInput) {
    if let Err(e) = run_loop_inner(&mut input).await {
        let _ = input.channel.send(AgentEvent::Error { message: e });
    }
}

async fn run_loop_inner(input: &mut LoopInput) -> Result<(), String> {
    let mut messages = std::mem::take(&mut input.messages);
    let mut turn = 0;

    // 注入 system prompt：默认 harness + 用户自定义（如有）
    // 仅当用户消息里没有自己的 system 消息时注入（避免重复）
    let has_system = messages
        .first()
        .map(|m| m.role == "system")
        .unwrap_or(false);
    if !has_system {
        let mut sys = input.default_system_prompt.clone();
        if let Some(user_prompt) = &input.system_prompt {
            if !user_prompt.trim().is_empty() {
                sys.push_str("\n\n# 用户自定义指令\n\n");
                sys.push_str(user_prompt.trim());
            }
        }
        messages.insert(0, LlmMessage::system(sys));
    }

    while turn < input.max_turns {
        turn += 1;

        // 每轮开头检查 cancel
        if input.cancel.is_cancelled() {
            return Ok(());
        }

        // 准备本轮 stream：text delta 通过 callback 推到前端
        let channel_for_text = input.channel.clone();
        let mut on_text = move |delta: &str| {
            let _ = channel_for_text.send(AgentEvent::TextDelta {
                text: delta.to_string(),
            });
        };

        // 安全校验
        let safe_endpoint =
            llm::validate_ai_request(&input.endpoint, &input.model, &input.api_key)?;
        let trimmed = super::trim::trim_conversation(&messages);
        let tools_slice = input.tools_schema.clone();

        let outcome = llm::stream_openai_request(StreamConfig {
            app: &input.app,
            endpoint: &safe_endpoint,
            api_key: &input.api_key,
            model: &input.model,
            messages: trimmed,
            tools: Some(&tools_slice),
            tool_choice: Some("auto"),
            on_text_delta: Some(&mut on_text),
            on_tool_calls_delta: None,
            chunk_event: "", // agent loop 不走旧 emit 路径
            done_event: "",
            request_id: "",
            abort_flag: None,
        })
        .await?;

        // 取消检查（stream 期间可能被 cancel）
        if input.cancel.is_cancelled() {
            return Ok(());
        }

        // 无 tool_calls：本轮结束
        if outcome.tool_calls.is_empty() {
            // 把 assistant 文本消息塞进历史（保持完整上下文）
            if !outcome.full_text.is_empty() {
                messages.push(LlmMessage::assistant(outcome.full_text.clone()));
            }
            let _ = input.channel.send(AgentEvent::Completed);
            return Ok(());
        }

        // 有 tool_calls：先把 assistant 消息（含 tool_calls）塞进历史
        let tool_calls_llm: Vec<LlmToolCall> =
            outcome.tool_calls.iter().map(LlmToolCall::from).collect();
        messages.push(LlmMessage {
            role: "assistant".into(),
            content: if outcome.full_text.is_empty() {
                None
            } else {
                Some(outcome.full_text)
            },
            tool_calls: Some(tool_calls_llm),
            tool_call_id: None,
        });

        // 逐个处理 tool_call
        for call in &outcome.tool_calls {
            if input.cancel.is_cancelled() {
                return Ok(());
            }
            process_tool_call(input, &mut messages, call).await;
        }
    }

    // 超过 MAX_TURNS
    let _ = input.channel.send(AgentEvent::Error {
        message: format!("已达到最大工具调用轮次限制（{} 次）", input.max_turns),
    });
    Ok(())
}

/// 处理单个 tool_call：审批 → 执行 → 回灌结果。
async fn process_tool_call(
    input: &mut LoopInput,
    messages: &mut Vec<LlmMessage>,
    call: &FinalizedToolCall,
) {
    // H14：args 经 scrub_secret 后再 emit 给前端，避免 LLM 在 ApprovalRequired / ToolCallArgs
    // 中复述用户 secret（UI v-html 渲染 / 审批弹窗展示）。call.arguments 是 LLM 构造的 JSON，
    // secret 不应出现于此；若出现则可能是 prompt injection 复述，打码更安全。
    let scrubbed_args = scrub_secret(&call.arguments.to_string()).into_owned();
    let scrubbed_args_value: serde_json::Value =
        serde_json::from_str(&scrubbed_args).unwrap_or(serde_json::Value::Null);

    // emit 工具调用开始事件
    let _ = input.channel.send(AgentEvent::ToolCallStart {
        id: call.id.clone(),
        name: call.name.clone(),
    });
    let _ = input.channel.send(AgentEvent::ToolCallArgs {
        id: call.id.clone(),
        args: scrubbed_args_value.clone(),
    });

    // 查找工具
    let Some(tool) = input.tool_registry.find(&call.name) else {
        let msg = format!("未知工具：{}", call.name);
        let _ = input.channel.send(AgentEvent::ToolResult {
            id: call.id.clone(),
            ok: false,
            output: msg.clone(),
        });
        messages.push(LlmMessage::tool_result(&call.id, msg));
        return;
    };

    // 审批检查
    let needs_approval = tool.requires_approval(&call.arguments);
    let approved = if needs_approval {
        // 用 tool_call.id 作 approval 索引（前端 part 路由 + agent_approve command 都用此 id）
        let rx = input.approval.create(call.id.clone());
        let _ = input.channel.send(AgentEvent::ApprovalRequired {
            id: call.id.clone(),
            tool_name: call.name.clone(),
            args: scrubbed_args_value.clone(),
        });
        // 等用户决定，同时监听 cancel + 超时（H13：避免用户不响应导致 loop 永久阻塞）
        let decision = select! {
            d = rx => d.unwrap_or(Decision::rejected()),
            _ = input.cancel.cancelled() => {
                // 用户取消（abort），告诉模型工具被中断
                let msg = "工具调用已被用户中断".to_string();
                let _ = input.channel.send(AgentEvent::ToolResult {
                    id: call.id.clone(),
                    ok: false,
                    output: msg.clone(),
                });
                messages.push(LlmMessage::tool_result(&call.id, msg));
                return;
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(APPROVAL_TIMEOUT_SECS)) => {
                // 超时自动拒绝：避免用户离开后 loop 永久阻塞、session 残留
                input.approval.resolve(&call.id, Decision::rejected());
                let msg = format!("审批超时（{} 秒未响应），已自动拒绝", APPROVAL_TIMEOUT_SECS);
                let _ = input.channel.send(AgentEvent::ToolResult {
                    id: call.id.clone(),
                    ok: false,
                    output: msg.clone(),
                });
                messages.push(LlmMessage::tool_result(&call.id, msg));
                return;
            }
        };
        if !decision.approved {
            let msg = "用户拒绝执行此工具".to_string();
            let _ = input.channel.send(AgentEvent::ToolResult {
                id: call.id.clone(),
                ok: false,
                output: msg.clone(),
            });
            messages.push(LlmMessage::tool_result(&call.id, msg));
            return;
        }
        // always_approve 由调用方（mod.rs / settings）读取持久化，此处仅放行
        true
    } else {
        true
    };

    if !approved {
        return;
    }

    // 执行工具（取消感知：abort 时 drop future，run_command 的 kill_on_drop 终结子进程）
    let result = select! {
        r = tool.call(call.arguments.clone()) => r,
        _ = input.cancel.cancelled() => {
            let msg = "工具调用已被用户中断".to_string();
            let _ = input.channel.send(AgentEvent::ToolResult {
                id: call.id.clone(),
                ok: false,
                output: msg.clone(),
            });
            messages.push(LlmMessage::tool_result(&call.id, msg));
            return;
        }
    };

    // 净化输出（防 secret 泄露给 LLM）
    let scrubbed = scrub_secret(&result.output).into_owned();

    let _ = input.channel.send(AgentEvent::ToolResult {
        id: call.id.clone(),
        ok: result.ok,
        output: scrubbed.clone(),
    });

    // 回灌给 LLM
    messages.push(LlmMessage::tool_result(&call.id, scrubbed));
}
