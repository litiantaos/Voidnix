/// Agent 扩展的前端类型定义。
///
/// 与 Rust 端 `AgentEvent` 枚举对齐（手写，不进 specta —— 动态 JSON 不支持）。
/// 调用方式：`invoke(CMD.agentRun, { ... })`，命令名常量见 src/commands.ts。

// ─── Agent 扩展推给前端的事件（通过 Channel<AgentEvent> 推送，Tauri ipc::Channel）────
export type AgentEvent =
  | { type: 'textDelta'; text: string }
  | { type: 'toolCallStart'; id: string; name: string }
  | { type: 'toolCallArgs'; id: string; args: unknown }
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

export interface WebSearchHit {
  title: string
  url: string
  snippet: string
}

export interface WebSearchResult {
  answer?: string
  hits: WebSearchHit[]
}

export type AgentPart =
  | { type: 'text'; text: string }
  | {
      type: 'toolCall'
      id: string
      name: string
      args?: unknown
      /** 工具执行结果（toolResult 事件填充；web_search 为 JSON 字符串供 LLM） */
      output?: string
      /** web_search 解析后的结构化结果（UI 渲染用） */
      parsed?: WebSearchResult
      /** 工具执行状态 */
      state: 'streaming' | 'running' | 'done' | 'failed'
    }
  /** UI 专属状态行（不进 LLM 上下文）：错误 / 用户中止 */
  | { type: 'notice'; kind: 'error' | 'aborted'; text: string }

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
// 注：invoke 参数在各调用点内联构造（agent 扩展自带 SearchProviderConfig），
//     无需集中 interface；Rust 端签名漂移由 check:commands CI + PR review 守护。
