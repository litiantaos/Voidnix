/// useAgentChat —— Agent 对话 composable。
///
/// 设计：
/// - 维护 `messages: Ref<AgentMessage[]>`（UI 状态层，parts 数组）
/// - sendMessage() 通过 `invoke('agent_run', { onEvent: Channel })` 启动
/// - Channel.onmessage 处理增量事件，更新 messages
/// - 用户审批通过 approve()/abort() 控制

import { ref, computed } from 'vue'
import { invoke, Channel } from '@tauri-apps/api/core'
import { useSettingsStore } from '@/stores/settings'
import { generateRequestId } from '@/utils/id'
import { config as agentConfig } from "./config"
import type { AgentEvent, AgentMessage, AgentPart, LlmMessage } from '@/types/agent'

export type AgentStatus = 'ready' | 'streaming' | 'awaiting_approval' | 'error'

export interface PendingApproval {
  approvalId: string
  toolCallId: string
  toolName: string
  args: unknown
  /** 决策回调（resolve 后前端弹窗消失） */
  resolve: (decision: { approved: boolean; alwaysApprove: boolean }) => void
}

const MAX_MESSAGES = 100

/// 单例式状态（一个 agent session 一次只跑一个）
const messages = ref<AgentMessage[]>([])
const status = ref<AgentStatus>('ready')
const errorMessage = ref('')
const sessionId = ref('')
const pendingApproval = ref<PendingApproval | null>(null)

export function useAgentChat() {
  const settings = useSettingsStore()

  const isGenerating = computed(
    () => status.value === 'streaming' || status.value === 'awaiting_approval',
  )

  /// 发送用户消息，启动一次 agent run
  async function sendMessage(text: string) {
    if (isGenerating.value || !text.trim()) return

    const config = settings.activeProviderConfig
    const key = settings.activeProviderModelKey
    const sep = key.indexOf('::')
    const model = sep !== -1 ? key.substring(sep + 2) : ''

    if (!config.endpoint || !config.apiKey) {
      messages.value.push({
        id: generateRequestId(),
        role: 'assistant',
        parts: [{ type: 'text', text: '请先在设置中配置 AI Provider 的 API 地址和 API Key。' }],
      })
      return
    }

    // 推入用户消息
    messages.value.push({
      id: generateRequestId(),
      role: 'user',
      parts: [{ type: 'text', text: text.trim() }],
    })
    trimHistory()

    // 构造 LlmMessage 数组（送 Rust 的格式）
    const llmMessages: LlmMessage[] = toLlmMessages(messages.value)

    // 准备 streaming assistant 消息
    const assistantId = generateRequestId()
    messages.value.push({
      id: assistantId,
      role: 'assistant',
      parts: [],
      streaming: true,
    })

    // 创建 Channel + 注册 onmessage
    const newSessionId = generateRequestId()
    sessionId.value = newSessionId
    const channel = new Channel<AgentEvent>()
    channel.onmessage = (msg) => handleEvent(msg, assistantId)

    status.value = 'streaming'
    errorMessage.value = ''

    const runConfig = {
      searchProvider: {
        type: settings.activeSearchProvider.type,
        apiKey: settings.activeSearchProvider.apiKey,
      },
      trustedCommands: agentConfig.trustedCommands,
      systemPrompt: agentConfig.systemPrompt || undefined,
    }

    try {
      await invoke('agent_run', {
        messages: llmMessages,
        endpoint: config.endpoint,
        apiKey: config.apiKey,
        model,
        sessionId: newSessionId,
        config: runConfig,
        onEvent: channel,
      })
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      errorMessage.value = msg
      status.value = 'error'
      finalizeStreamingMessage(assistantId)
      // 推入错误消息
      messages.value.push({
        id: generateRequestId(),
        role: 'assistant',
        parts: [{ type: 'text', text: `错误：${msg}` }],
      })
    }
  }

  /// 处理 AgentEvent，更新 messages
  function handleEvent(event: AgentEvent, assistantId: string) {
    const msg = findMessage(assistantId)
    if (!msg) return

    switch (event.type) {
      case 'textDelta': {
        // 找最后一个 text part 或新建
        const lastPart = msg.parts[msg.parts.length - 1]
        if (lastPart && lastPart.type === 'text') {
          lastPart.text += event.text
        } else {
          msg.parts.push({ type: 'text', text: event.text })
        }
        break
      }
      case 'toolCallStart': {
        msg.parts.push({
          type: 'toolCall',
          id: event.id,
          name: event.name,
          state: 'streaming',
        })
        break
      }
      case 'toolCallArgs': {
        const part = findToolPart(msg, event.id)
        if (part) {
          part.args = event.args
          // 保持 streaming 状态：等 ApprovalRequired（→ awaiting）或 ToolResult（→ done/failed）
          // 不立即设 running，避免与审批弹窗同时出现时显示矛盾
        }
        break
      }
      case 'approvalRequired': {
        const part = findToolPart(msg, event.id)
        if (part) part.state = 'awaiting_approval'
        status.value = 'awaiting_approval'
        pendingApproval.value = {
          approvalId: event.id,
          toolCallId: event.id,
          toolName: event.toolName,
          args: event.args,
          resolve: () => {
            pendingApproval.value = null
          },
        }
        break
      }
      case 'toolResult': {
        const part = findToolPart(msg, event.id)
        if (part) {
          part.output = event.output
          part.ok = event.ok
          part.state = event.ok ? 'done' : 'failed'
        }
        if (status.value === 'awaiting_approval') status.value = 'streaming'
        break
      }
      case 'completed': {
        finalizeStreamingMessage(assistantId)
        status.value = 'ready'
        break
      }
      case 'error': {
        finalizeStreamingMessage(assistantId)
        errorMessage.value = event.message
        status.value = 'error'
        break
      }
    }
  }

  function findMessage(id: string): AgentMessage | undefined {
    return messages.value.find((m) => m.id === id)
  }

  function findToolPart(
    msg: AgentMessage,
    toolCallId: string,
  ): Extract<AgentPart, { type: 'toolCall' }> | undefined {
    return msg.parts.find(
      (p): p is Extract<AgentPart, { type: 'toolCall' }> =>
        p.type === 'toolCall' && p.id === toolCallId,
    )
  }

  function finalizeStreamingMessage(id: string) {
    const msg = findMessage(id)
    if (msg) {
      msg.streaming = false
      // 移除空 parts（streaming 但没收到任何内容）
      msg.parts = msg.parts.filter((p) => {
        if (p.type === 'text') return p.text.length > 0
        return true
      })
    }
  }

  /// 回复审批
  async function approve(approved: boolean, alwaysApprove: boolean) {
    if (!pendingApproval.value) return
    const { approvalId, toolName, resolve } = pendingApproval.value
    pendingApproval.value = null
    status.value = 'streaming'

    try {
      await invoke('agent_approve', {
        approvalId,
        approved,
        alwaysApprove,
      })
      // 持久化「执行并信任」
      if (approved && alwaysApprove && toolName) {
        agentConfig.trustedCommands = [...agentConfig.trustedCommands, toolName]
      }
    } catch (e) {
      errorMessage.value = e instanceof Error ? e.message : String(e)
    } finally {
      resolve({ approved, alwaysApprove })
    }
  }

  /// 中断当前 agent run
  async function abort() {
    if (!sessionId.value) return
    try {
      await invoke('agent_abort', { sessionId: sessionId.value })
    } catch {
      /* ignore */
    }
    status.value = 'ready'
    sessionId.value = ''
    pendingApproval.value = null

    // finalize 当前 streaming 消息
    const streamingMsg = messages.value.find((m) => m.streaming)
    if (streamingMsg) finalizeStreamingMessage(streamingMsg.id)
  }

  /// 清空对话
  function newConversation() {
    if (isGenerating.value) abort()
    messages.value = []
    status.value = 'ready'
    errorMessage.value = ''
    sessionId.value = ''
    pendingApproval.value = null
  }

  function trimHistory() {
    if (messages.value.length <= MAX_MESSAGES) return
    messages.value = messages.value.slice(-MAX_MESSAGES)
  }

  return {
    messages,
    status,
    isGenerating,
    errorMessage,
    pendingApproval,
    sendMessage,
    approve,
    abort,
    newConversation,
  }
}

/// AgentMessage[] → LlmMessage[]（送 Rust 的 OpenAI 协议格式）
function toLlmMessages(messages: AgentMessage[]): LlmMessage[] {
  const result: LlmMessage[] = []
  for (const msg of messages) {
    if (msg.streaming) continue // 跳过未完成的 streaming 消息
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
