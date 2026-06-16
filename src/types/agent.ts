/// Agent 扩展的前端类型定义。
///
/// 与 Rust 端 `AgentEvent` 枚举对齐（手写，不进 specta —— 动态 JSON 不支持）。
/// 调用方式：`invoke('agent_run', { ... })` 裸 invoke，不通过 bindings.ts。

import type { Channel } from '@tauri-apps/api/core'

// ─── Agent 扩展推给前端的事件（Channel<T> 中的 T）────────────
export type AgentEvent =
  | { type: 'textDelta'; text: string }
  | { type: 'toolCallStart'; id: string; name: string }
  | { type: 'toolCallArgs'; id: string; args: unknown }
  | { type: 'approvalRequired'; id: string; toolName: string; args: unknown }
  | { type: 'toolResult'; id: string; ok: boolean; output: string }
  | { type: 'completed' }
  | { type: 'error'; message: string }

// ─── UI 消息层 ───────────────────────────────────────────────
export interface AgentMessage {
  id: string
  role: 'user' | 'assistant' | 'tool'
  parts: AgentPart[]
  /** 是否正在 streaming（assistant message 临时标记） */
  streaming?: boolean
}

export type AgentPart =
  | { type: 'text'; text: string }
  | {
      type: 'toolCall'
      id: string
      name: string
      args?: unknown
      /** 工具执行状态 */
      state: 'streaming' | 'awaiting_approval' | 'running' | 'done' | 'failed'
      output?: string
      ok?: boolean
      /** 待审批 ID（state=awaiting_approval 时有） */
      approvalId?: string
    }

// ─── LLM 协议层消息（Rust `LlmMessage` 的前端映射）────────────
export interface LlmMessage {
  role: 'system' | 'user' | 'assistant' | 'tool'
  content?: string
  toolCalls?: Array<{
    id: string
    type: 'function'
    function: { name: string; arguments: string }
  }>
  toolCallId?: string
}

// ─── invoke 参数类型 ────────────────────────────────────────
export interface SearchProviderConfig {
  type: 'tavily'
  apiKey: string
}

export interface AgentRunConfig {
  searchProvider: SearchProviderConfig
  trustedCommands: string[]
  systemPrompt?: string
}

export interface AgentRunArgs {
  messages: LlmMessage[]
  endpoint: string
  apiKey: string
  model: string
  sessionId: string
  config: AgentRunConfig
  onEvent: Channel<AgentEvent>
}

export interface AgentApproveArgs {
  /// 即 tool_call.id（前端 part 路由用的同一 id）
  approvalId: string
  approved: boolean
  alwaysApprove: boolean
}

export interface AgentAbortArgs {
  sessionId: string
}
