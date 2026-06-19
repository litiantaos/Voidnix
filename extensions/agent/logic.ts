import type { AgentMessage, AgentPart, LlmMessage } from '@/types/agent'

/// AgentMessage[] → LlmMessage[]（送 Rust 的 OpenAI 协议格式）。
///
/// - 跳过 streaming 消息（未完成，不进上下文）
/// - user：合并所有 text part 为 content
/// - assistant：text part 合并为 content，toolCall part 序列化为 toolCalls
/// - 空内容（无 text 且无 toolCall）的消息跳过
export function toLlmMessages(messages: AgentMessage[]): LlmMessage[] {
  const result: LlmMessage[] = []
  for (const msg of messages) {
    if (msg.streaming) continue
    if (msg.role === 'user') {
      const text = msg.parts
        .filter((p): p is Extract<AgentPart, { type: 'text' }> => p.type === 'text')
        .map((p) => p.text)
        .join('')
      if (text) result.push({ role: 'user', content: text })
    } else if (msg.role === 'assistant') {
      const textParts = msg.parts
        .filter((p): p is Extract<AgentPart, { type: 'text' }> => p.type === 'text')
        .map((p) => p.text)
        .join('')
      const toolCalls = msg.parts
        .filter((p): p is Extract<AgentPart, { type: 'toolCall' }> => p.type === 'toolCall')
        .map((p) => ({
          id: p.id,
          type: 'function' as const,
          function: { name: p.name, arguments: JSON.stringify(p.args ?? {}) },
        }))
      if (textParts || toolCalls.length > 0) {
        const entry: LlmMessage = { role: 'assistant' }
        if (textParts) entry.content = textParts
        if (toolCalls.length > 0) entry.toolCalls = toolCalls
        result.push(entry)
      }
    }
    // tool role 在 history 里不出现（tool result 在 assistant 的 toolCall part 上）
  }
  return result
}
