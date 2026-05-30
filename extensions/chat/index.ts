import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { registerModule } from '@/core/module-registry'
import { asyncView } from '@/core/async-view'
import { moduleSelfResult, makeToggleHandler } from '@/core/module-helpers'
import type { AppModule } from '@/types/module'
import { useSettingsStore } from '@/stores/settings'
import { toErrorMessage } from '@/utils/error'
import { generateRequestId } from '@/composables/useStreamOutput'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

const ChatView = asyncView(() => import('./View.vue'))
const ChatSettings = asyncView(() => import('./Settings.vue'))
const ChatActions = asyncView(() => import('./Actions.vue'))

export interface ChatMessage {
  role: 'system' | 'user' | 'assistant'
  content: string
}

export interface ChatConversation {
  id: string
  messages: ChatMessage[]
  createdAt: number
}

/** 前端消息历史硬上限（与 Rust 端 MAX_CONVERSATION_MESSAGES 对齐） */
const MAX_MESSAGES = 100

// 当前对话
export const currentConversation = ref<ChatConversation>({
  id: generateRequestId(),
  messages: [],
  createdAt: Date.now(),
})

// 是否正在生成
export const isGenerating = ref(false)

// 当前流式消息
export const streamingMessage = ref('')

// 监听器句柄
let unlistenChunk: UnlistenFn | null = null
let unlistenDone: UnlistenFn | null = null
let initializing = false

// 当前请求 ID
let currentRequestId = ''


// 初始化监听器
export async function initListeners() {
  if (unlistenChunk || initializing) return
  initializing = true

  try {
    unlistenChunk = await listen<{ requestId: string; content: string }>(
      'chat-chunk',
      (event) => {
        if (
          isGenerating.value &&
          event.payload.requestId === currentRequestId
        ) {
          streamingMessage.value += event.payload.content
        }
      },
    )

    unlistenDone = await listen<{ requestId: string }>('chat-done', (event) => {
      if (event.payload.requestId === currentRequestId) {
        if (streamingMessage.value) {
          currentConversation.value.messages.push({
            role: 'assistant',
            content: streamingMessage.value,
          })
          streamingMessage.value = ''
        }
        isGenerating.value = false
        currentRequestId = ''
      }
    })
  } finally {
    initializing = false
  }
}

/** 裁剪消息历史到安全上限 */
function trimHistory() {
  const msgs = currentConversation.value.messages
  if (msgs.length <= MAX_MESSAGES) return

  const systemMsg = msgs.find((m) => m.role === 'system')
  const recent = msgs.slice(-(MAX_MESSAGES - 1))
  currentConversation.value.messages = systemMsg
    ? [systemMsg, ...recent.filter((m) => m !== systemMsg)]
    : recent
}

// 发送消息
export async function sendMessage(content: string) {
  if (!content.trim() || isGenerating.value) return

  const settings = useSettingsStore()
  const config = settings.activeChatConfig
  const key = settings.activeModelKey
  const sep = key.indexOf('::')
  const activeModel = sep !== -1 ? key.substring(sep + 2) : ''

  if (!config.endpoint || !config.apiKey) {
    currentConversation.value.messages.push({
      role: 'assistant',
      content: '请先在设置中配置 AI Chat 的 API 地址和 API Key。',
    })
    return
  }

  currentConversation.value.messages.push({
    role: 'user',
    content: content.trim(),
  })

  trimHistory()

  const messages: ChatMessage[] = [...currentConversation.value.messages]

  currentRequestId = generateRequestId()

  isGenerating.value = true
  streamingMessage.value = ''

  try {
    await invoke('chat_stream', {
      messages,
      endpoint: config.endpoint,
      apiKey: config.apiKey,
      model: activeModel,
      requestId: currentRequestId,
    })
  } catch (e) {
    console.error('Chat stream error:', e)
    isGenerating.value = false
    streamingMessage.value = ''
    currentRequestId = ''
    const errorMsg = toErrorMessage(e, '未知错误，请检查 API 配置和网络连接')
    currentConversation.value.messages.push({
      role: 'assistant',
      content: `错误: ${errorMsg}`,
    })
  }
}

/** 清理事件监听器 */
export function destroyListeners() {
  unlistenChunk?.()
  unlistenChunk = null
  unlistenDone?.()
  unlistenDone = null
  initializing = false
}

// 新建对话
export function newConversation() {
  currentConversation.value = {
    id: generateRequestId(),
    messages: [],
    createdAt: Date.now(),
  }
  streamingMessage.value = ''
  isGenerating.value = false
  currentRequestId = ''
}

// 停止生成
export function stopGenerating() {
  invoke('chat_abort').catch(() => {})
  isGenerating.value = false

  if (streamingMessage.value) {
    currentConversation.value.messages.push({
      role: 'assistant',
      content: streamingMessage.value,
    })
    streamingMessage.value = ''
  }
  currentRequestId = ''
}

const mod: AppModule = {
  id: 'chat',
  name: 'AI Chat',
  description: 'AI 对话扩展',
  icon: 'i-ri-chat-ai-line',
  keywords: ['chat', 'ai', 'gpt', '对话', '聊天', '助手', 'assistant'],
  order: 9,
  disableSearchInput: true,
  layout: { view: ChatView, searchBarAccessory: ChatActions },
  panel: ChatSettings,
  globalShortcuts: [
    {
      id: 'chat',
      default: 'CommandOrControl+Shift+A',
      onExecute: makeToggleHandler('chat'),
    },
  ],
  onInit: async () => {
    await initListeners()
  },
  onSearch: async (query) => {
    if (!query.trim()) return []
    const q = query.toLowerCase()
    if (
      q.includes('chat') ||
      q.includes('ai') ||
      q.includes('对话') ||
      q.includes('聊天')
    ) {
      return [moduleSelfResult(mod)]
    }
    return []
  },
  onModuleSearch: async () => {
    return []
  },
  onExecute: async () => {},
}

registerModule(mod)