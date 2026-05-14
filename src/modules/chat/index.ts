import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { registerModule } from '@/core/module-registry'
import type { AppModule } from '@/types/module'
import { useSettingsStore } from '@/stores/settings'
import ChatView from './ChatView.vue'
import ChatSettings from './ChatSettings.vue'
import ChatToolbar from './ChatToolbar.vue'

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
  id: generateId(),
  messages: [],
  createdAt: Date.now(),
})

// 是否正在生成
export const isGenerating = ref(false)

// 当前流式消息
export const streamingMessage = ref('')

// 监听器句柄
let unlistenChunk: (() => void) | null = null
let unlistenDone: (() => void) | null = null
let initializing = false

function generateId(): string {
  return Date.now().toString(36) + Math.random().toString(36).substr(2)
}

// 初始化监听器
export async function initListeners() {
  if (unlistenChunk || initializing) return
  initializing = true

  try {
    unlistenChunk = await listen<{ content: string }>('chat-chunk', (event) => {
      if (isGenerating.value) {
        // 直接追加，由 vue-stream-markdown 的 streaming mode 处理增量动画
        streamingMessage.value += event.payload.content
      }
    })

    // 保存 unlisten 句柄防止泄漏
    unlistenDone = await listen('chat-done', () => {
      if (streamingMessage.value) {
        currentConversation.value.messages.push({
          role: 'assistant',
          content: streamingMessage.value,
        })
        streamingMessage.value = ''
      }
      isGenerating.value = false
    })
  } finally {
    initializing = false
  }
}

/** 裁剪消息历史到安全上限 */
function trimHistory() {
  const msgs = currentConversation.value.messages
  if (msgs.length <= MAX_MESSAGES) return

  // 保留 system 消息 + 最近的消息
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

  // 检查配置
  if (!config.endpoint || !config.apiKey) {
    currentConversation.value.messages.push({
      role: 'assistant',
      content: '请先在设置中配置 AI Chat 的 API 地址和 API Key。',
    })
    return
  }

  // 添加用户消息
  currentConversation.value.messages.push({
    role: 'user',
    content: content.trim(),
  })

  // 裁剪历史（belt-and-suspenders：前端裁剪 + Rust 端裁剪）
  trimHistory()

  // 准备消息列表
  const messages: ChatMessage[] = [...currentConversation.value.messages]

  // 开始生成
  isGenerating.value = true
  streamingMessage.value = ''

  try {
    await invoke('chat_stream', {
      messages,
      endpoint: config.endpoint,
      apiKey: config.apiKey,
      model: activeModel,
    })
  } catch (e) {
    console.error('Chat stream error:', e)
    isGenerating.value = false
    streamingMessage.value = ''
    // 仅显示用户友好的错误信息，不泄露原始异常
    const errorMsg =
      e instanceof Error
        ? e.message || '未知错误，请检查 API 配置和网络连接'
        : '请求失败，请检查 API 配置和网络连接'
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
    id: generateId(),
    messages: [],
    createdAt: Date.now(),
  }
  streamingMessage.value = ''
  isGenerating.value = false
}

// 停止生成：忽略后续到达的流式数据，将当前缓冲内容作为最终结果
export function stopGenerating() {
  isGenerating.value = false

  if (streamingMessage.value) {
    currentConversation.value.messages.push({
      role: 'assistant',
      content: streamingMessage.value,
    })
    streamingMessage.value = ''
  }
}

const mod: AppModule = {
  id: 'chat',
  name: 'AI Chat',
  description: 'AI 对话扩展',
  icon: 'i-ri-chat-ai-line',
  keywords: ['chat', 'ai', 'gpt', '对话', '聊天', '助手', 'assistant'],
  order: 9,
  layout: { view: ChatView },
  settings: ChatSettings,
  toolbar: ChatToolbar,
  multiline: true,
  onInit: async () => {
    await initListeners()
  },
  onSearch: async (query) => {
    // 修复：query.includes('chat') 而非 'chat'.includes(query)
    const q = query.toLowerCase()
    if (
      q.includes('chat') ||
      q.includes('ai') ||
      q.includes('对话') ||
      q.includes('聊天')
    ) {
      return [
        {
          id: 'chat-module',
          title: 'AI Chat',
          description: '打开 AI 对话扩展',
          module: 'chat',
          icon: 'i-ri-chat-ai-line',
          score: 100,
          data: { kind: 'module', moduleId: 'chat' },
        },
      ]
    }
    return []
  },
  onModuleSearch: async () => {
    return []
  },
  onExecute: async () => {},
}

registerModule(mod)
