import type { AgentMessage, AgentPart, LlmMessage } from '@/types/agent'

/// AgentMessage[] → LlmMessage[]（送 Rust 的 OpenAI 协议格式）。
///
/// - 跳过 streaming 消息（未完成，不进上下文）
/// - user：合并所有 text part 为 content
/// - assistant：text part 合并为 content，toolCall part 序列化为 toolCalls；
///   每个 toolCall 后跟一条 role:tool 结果消息（OpenAI 协议要求 tool_calls 必须有对应 result）
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
      const toolCallParts = msg.parts.filter(
        (p): p is Extract<AgentPart, { type: 'toolCall' }> => p.type === 'toolCall',
      )
      if (textParts || toolCallParts.length > 0) {
        const entry: LlmMessage = { role: 'assistant' }
        if (textParts) entry.content = textParts
        if (toolCallParts.length > 0) {
          entry.toolCalls = toolCallParts.map((p) => ({
            id: p.id,
            type: 'function' as const,
            function: { name: p.name, arguments: JSON.stringify(p.args ?? {}) },
          }))
        }
        result.push(entry)
        // OpenAI 协议：assistant 的 tool_calls 后必须跟每条 tool 结果
        for (const tc of toolCallParts) {
          result.push({
            role: 'tool',
            toolCallId: tc.id,
            content: tc.output || '(无输出)',
          })
        }
      }
    }
  }
  return result
}

/// 解析 web_search 的 output（JSON），提取 answer 摘要。无 answer 或解析失败返回 undefined。
export function tryParseSearchAnswer(output: string): string | undefined {
  try {
    const obj = JSON.parse(output)
    if (!obj || typeof obj !== 'object') return undefined
    const answer = typeof obj.answer === 'string' ? obj.answer.trim() : ''
    return answer || undefined
  } catch {
    return undefined
  }
}
