//! Agent loop runner —— 后台 task 主循环。
//!
//! 数据流：
//! ```
//! loop {
//!   stream_openai_request(messages, tools) → outcome
//!   match outcome {
//!     Text(s)     → emit Completed, break
//!     ToolCalls(c)→ for each call {
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

use crate::extensions::agent::engine::tool_registry::ToolRegistry;
use crate::extensions::agent::engine::AgentEvent;
use crate::runtime::llm::parser::FinalizedToolCall;
use crate::runtime::llm::{self, LlmMessage, LlmToolCall, StreamConfig};

use super::secret_scrub::scrub_secret;

/// Agent loop 的输入配置。
pub struct LoopInput {
    pub app: tauri::AppHandle,
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
    pub messages: Vec<LlmMessage>,
    /// system prompt（前端 config 直传，空串则不注入 system 消息）。
    pub system_prompt: String,
    /// 单次 agent run 的最多工具调用轮次（防失控）。
    pub max_turns: usize,
    pub tools_schema: Vec<serde_json::Value>,
    pub tool_registry: Arc<ToolRegistry>,
    pub channel: Channel<AgentEvent>,
    pub cancel: CancellationToken,
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

    // 注入 system prompt（前端 config 直传，空串跳过）
    // 仅当用户消息里没有自己的 system 消息时注入（避免重复）
    let has_system = messages
        .first()
        .map(|m| m.role == "system")
        .unwrap_or(false);
    if !has_system {
        let sys = input.system_prompt.trim();
        if !sys.is_empty() {
            messages.insert(0, LlmMessage::system(sys.to_string()));
        }
    }

    // 安全校验：endpoint/model/key 在单次 run 内不变，循环外一次校验
    let safe_endpoint = llm::validate_ai_request(&input.endpoint, &input.model, &input.api_key)?;
    // tools_schema 循环内不变，循环外克隆一次
    let tools_schema = input.tools_schema.clone();

    while turn < input.max_turns {
        turn += 1;

        // 每轮开头检查 cancel
        if input.cancel.is_cancelled() {
            return Ok(());
        }

        // 准备本轮 stream：text / reasoning delta 通过 callback 推到前端
        let channel_for_text = input.channel.clone();
        let mut on_text = move |delta: &str| {
            let _ = channel_for_text.send(AgentEvent::TextDelta {
                text: delta.to_string(),
            });
        };
        let channel_for_reasoning = input.channel.clone();
        let mut on_reasoning = move |delta: &str| {
            let _ = channel_for_reasoning.send(AgentEvent::ReasoningDelta {
                text: delta.to_string(),
            });
        };

        // 安全校验（循环外已完成，此处复用）
        let trimmed = super::trim::trim_conversation(&messages);

        let outcome = llm::stream_openai_request(StreamConfig {
            app: &input.app,
            endpoint: &safe_endpoint,
            api_key: &input.api_key,
            model: &input.model,
            messages: &trimmed,
            tools: Some(&tools_schema),
            tool_choice: Some("auto"),
            on_text_delta: Some(&mut on_text),
            on_reasoning_delta: Some(&mut on_reasoning),
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

/// 处理单个 tool_call：执行 → 回灌结果。
async fn process_tool_call(
    input: &mut LoopInput,
    messages: &mut Vec<LlmMessage>,
    call: &FinalizedToolCall,
) {
    // H14：args 经 scrub_secret 后再 emit 给前端，避免 LLM 在 ToolCallArgs
    // 中复述用户 secret（UI v-html 渲染）。call.arguments 是 LLM 构造的 JSON，
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
