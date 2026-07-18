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
import { showToast } from '@/composables/useToast'
import { config as agentConfig, resolveAgentRuntimeCredentials } from './config'
import type { AgentEvent, AgentMessage, AgentPart, LlmMessage } from '@/types/agent'
import { toLlmMessages, tryParseSearch } from './logic'

export type AgentStatus = 'ready' | 'streaming' | 'error'

const MAX_MESSAGES = 100

/// 单例式状态（一个 agent session 一次只跑一个）
const messages = ref<AgentMessage[]>([])
const status = ref<AgentStatus>('ready')
const sessionId = ref('')

export function useAgentChat() {
  const isGenerating = computed(() => status.value === 'streaming')

  /// 发送用户消息，启动一次 agent run
  async function sendMessage(text: string) {
    if (isGenerating.value || !text.trim()) return

    // 本扩展自选模型；配置优先，缺项回退进程 env / ~/.config/voidnix/ai.env
    const creds = await resolveAgentRuntimeCredentials()
    if (!creds) {
      messages.value.push({
        id: generateRequestId(),
        role: 'assistant',
        parts: [
          {
            type: 'notice',
            kind: 'error',
            text: '请先在「AI 提供商」配置 API 并在本扩展选择模型，或设置 OPENAI_API_KEY / OPENAI_BASE_URL / OPENAI_MODEL（shell source ai.env）。',
          },
        ],
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
        endpoint: creds.endpoint,
        apiKey: creds.apiKey,
        model: creds.model,
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
  /// 内容侧：仅 `streaming` 气泡可写（abort/完成/错误 finalize 后拒绝晚到 delta/tool）。
  /// 控制面：status/sessionId 仅当仍是 runSessionId 时才改。
  function handleEvent(event: AgentEvent, assistantId: string, runSessionId: string) {
    const msg = findMessage(assistantId)
    if (!msg) return

    // 已收尾的气泡：忽略晚到内容事件（中止后 textDelta 不得接在「已中止」后）
    const contentEvent =
      event.type === 'textDelta' ||
      event.type === 'toolCallStart' ||
      event.type === 'toolCallArgs' ||
      event.type === 'toolResult'
    if (contentEvent && !msg.streaming) return

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
        if (msg.streaming) finalizeStreamingMessage(assistantId)
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

  /// 错误写入 assistant notice part 并结束 streaming。
  /// 内容仅在仍 streaming 时写入；控制面仅当 session 仍属本 run 时改。
  function pushErrorOnMessage(assistantId: string, message: string, runSessionId?: string) {
    const msg = findMessage(assistantId)
    if (msg?.streaming) {
      failInFlightTools(msg)
      msg.parts.push({ type: 'notice', kind: 'error', text: message })
      finalizeStreamingMessage(assistantId)
    }
    if (runSessionId === undefined || sessionId.value === runSessionId) {
      status.value = 'error'
      sessionId.value = ''
      showToast(message, { kind: 'error' })
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

  /** 进行中的工具标 failed，避免中止/错误后残留 shimmer */
  function failInFlightTools(msg: AgentMessage) {
    for (const p of msg.parts) {
      if (p.type === 'toolCall' && (p.state === 'streaming' || p.state === 'running')) {
        p.state = 'failed'
      }
    }
  }

  function finalizeStreamingMessage(id: string) {
    const msg = findMessage(id)
    if (msg) {
      msg.streaming = false
      // 移除空 text parts（streaming 但没收到任何内容）；notice / tool 保留
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

    const streamingMsg = messages.value.find((m) => m.streaming)
    if (streamingMsg) {
      failInFlightTools(streamingMsg)
      streamingMsg.parts.push({ type: 'notice', kind: 'aborted', text: '已中止' })
      finalizeStreamingMessage(streamingMsg.id)
    }
  }

  /// 清空对话
  async function newConversation() {
    if (isGenerating.value) await abort()
    messages.value = []
    status.value = 'ready'
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
    sendMessage,
    abort,
    newConversation,
  }
}
