/// useAgentChat —— Agent 对话 composable。
///
/// 设计：
/// - 维护 `messages: Ref<AgentMessage[]>`（UI 状态层，parts 数组）
/// - sendMessage() 通过 `invoke(CMD.agentRun, { onEvent: Channel })` 启动
/// - Channel.onmessage 处理增量事件，更新 messages
/// - 用户中断通过 abort()

import { ref, computed } from 'vue'
import { invoke, Channel } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { generateRequestId } from '@/utils/id'
import {
  config as agentConfig,
  activeProviderConfig,
  resolveActiveModel,
  isProviderReady,
} from './config'
import type { AgentEvent, AgentMessage, AgentPart, LlmMessage } from '@/types/agent'
import { toLlmMessages, tryParseSearch } from './logic'

export type AgentStatus = 'ready' | 'streaming' | 'error'

const MAX_MESSAGES = 100

/// 单例式状态（一个 agent session 一次只跑一个）
const messages = ref<AgentMessage[]>([])
const status = ref<AgentStatus>('ready')
const errorMessage = ref('')
const sessionId = ref('')

export function useAgentChat() {
  const isGenerating = computed(() => status.value === 'streaming')

  /// 发送用户消息，启动一次 agent run
  async function sendMessage(text: string) {
    if (isGenerating.value || !text.trim()) return

    const provider = activeProviderConfig.value
    const model = resolveActiveModel()

    if (!isProviderReady.value) {
      const hint =
        !provider.endpoint.trim() || !provider.apiKey.trim()
          ? '请先在设置中配置 AI Provider 的 API 地址和 API Key。'
          : '请先在设置中选择模型。'
      messages.value.push({
        id: generateRequestId(),
        role: 'assistant',
        parts: [{ type: 'text', text: hint }],
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

    // 创建 Channel + 注册 onmessage（runSessionId 闭包：晚到事件不得踩踏新 run 控制面）
    const newSessionId = generateRequestId()
    sessionId.value = newSessionId
    const channel = new Channel<AgentEvent>()
    channel.onmessage = (msg) => handleEvent(msg, assistantId, newSessionId)

    status.value = 'streaming'
    errorMessage.value = ''

    const runConfig = {
      searchProvider: {
        type: agentConfig.searchProvider.type,
        apiKey: agentConfig.searchProvider.apiKey,
      },
      maxCpuSeconds: agentConfig.maxCpuSeconds,
      maxMemoryMb: agentConfig.maxMemoryMb,
      maxOpenFiles: agentConfig.maxOpenFiles,
      executionTimeout: agentConfig.executionTimeout,
      maxOutputBytes: agentConfig.maxOutputBytes,
      maxTurns: agentConfig.maxTurns,
      systemPrompt: agentConfig.systemPrompt,
    }

    try {
      await invoke(CMD.agentRun, {
        messages: llmMessages,
        endpoint: provider.endpoint,
        apiKey: provider.apiKey,
        model,
        sessionId: newSessionId,
        config: runConfig,
        onEvent: channel,
      })
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      pushErrorOnMessage(assistantId, msg, newSessionId)
    }
  }

  /// 处理 AgentEvent，更新 messages。
  /// 内容侧按 assistantId 写旧气泡；status/sessionId 仅当仍是 runSessionId 时才改。
  function handleEvent(event: AgentEvent, assistantId: string, runSessionId: string) {
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
          // 参数就绪 → 进入执行中（直接执行，无审批）
          part.state = 'running'
        }
        break
      }
      case 'toolResult': {
        const part = findToolPart(msg, event.id)
        if (part) {
          part.state = event.ok ? 'done' : 'failed'
          part.output = event.output
          // web_search：成功解析结构化结果供 UI hits；失败时 parsed 空，UI 展示 output
          part.parsed =
            part.name === 'web_search' && event.ok ? tryParseSearch(event.output) : undefined
        }
        break
      }
      case 'completed': {
        finalizeStreamingMessage(assistantId)
        if (sessionId.value === runSessionId) {
          status.value = 'ready'
          sessionId.value = ''
        }
        break
      }
      case 'error': {
        pushErrorOnMessage(assistantId, event.message, runSessionId)
        break
      }
    }
  }

  /// 错误写入 assistant 气泡并结束 streaming。
  /// 仅当 `sessionId` 仍属本 run（或未传 runSessionId）时改全局 status/sessionId。
  function pushErrorOnMessage(assistantId: string, message: string, runSessionId?: string) {
    const msg = findMessage(assistantId)
    if (msg) {
      msg.parts.push({ type: 'text', text: `错误：${message}` })
    }
    finalizeStreamingMessage(assistantId)
    if (runSessionId === undefined || sessionId.value === runSessionId) {
      errorMessage.value = message
      status.value = 'error'
      sessionId.value = ''
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

  /// 中断当前 agent run
  async function abort() {
    if (!sessionId.value) return
    try {
      await invoke(CMD.agentAbort, { sessionId: sessionId.value })
    } catch {
      /* ignore */
    }
    status.value = 'ready'
    sessionId.value = ''

    // finalize 当前 streaming 消息
    const streamingMsg = messages.value.find((m) => m.streaming)
    if (streamingMsg) finalizeStreamingMessage(streamingMsg.id)
  }

  /// 清空对话
  async function newConversation() {
    if (isGenerating.value) await abort()
    messages.value = []
    status.value = 'ready'
    errorMessage.value = ''
    sessionId.value = ''
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
    sendMessage,
    abort,
    newConversation,
  }
}
