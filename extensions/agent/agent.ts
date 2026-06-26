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
import { useSettingsStore } from '@/stores/settings'
import { generateRequestId } from '@/utils/id'
import { config as agentConfig } from './config'
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
  const settings = useSettingsStore()

  const isGenerating = computed(() => status.value === 'streaming')

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
          // web_search 解析结构化结果供 UI 卡片渲染
          part.parsed =
            part.name === 'web_search' && event.ok ? tryParseSearch(event.output) : undefined
        }
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
  function newConversation() {
    if (isGenerating.value) abort()
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
