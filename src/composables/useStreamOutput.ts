import { ref, onUnmounted } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

export interface StreamChunkPayload {
  requestId: string
  content: string
}

export interface StreamDonePayload {
  requestId: string
}

export interface StreamOptions {
  requestId: string
  chunkEvent: string
  doneEvent: string
  onChunk: (content: string) => void
  onDone: () => void
  onError?: (error: string) => void
}

interface StreamSession {
  unlistenChunk: UnlistenFn
  unlistenDone: UnlistenFn
  unlistenError?: UnlistenFn
}

const activeSessions = new Map<string, StreamSession>()

export function useStreamOutput() {
  const isStreaming = ref(false)
  const streamingContent = ref('')

  async function startStream(options: StreamOptions): Promise<void> {
    const { requestId, chunkEvent, doneEvent, onChunk, onDone, onError } = options

    // 清理已存在的同 requestId 监听器
    stopStream(requestId)

    isStreaming.value = true

    const unlistenChunk = await listen<StreamChunkPayload>(chunkEvent, (event) => {
      if (event.payload.requestId === requestId) {
        const content = event.payload.content
        streamingContent.value += content
        onChunk(content)
      }
    })

    const unlistenDone = await listen<StreamDonePayload>(doneEvent, (event) => {
      if (event.payload.requestId === requestId) {
        cleanupSession(requestId)
        isStreaming.value = false
        onDone()
      }
    })

    let unlistenError: UnlistenFn | undefined
    if (onError) {
      unlistenError = await listen<{ requestId: string; error: string }>(
        `${chunkEvent}-error`,
        (event) => {
          if (event.payload.requestId === requestId) {
            cleanupSession(requestId)
            isStreaming.value = false
            onError(event.payload.error)
          }
        },
      )
    }

    activeSessions.set(requestId, { unlistenChunk, unlistenDone, unlistenError })
  }

  function stopStream(requestId: string): void {
    const session = activeSessions.get(requestId)
    if (session) {
      session.unlistenChunk()
      session.unlistenDone()
      session.unlistenError?.()
      activeSessions.delete(requestId)
      isStreaming.value = false
    }
  }

  function cleanupSession(requestId: string): void {
    const session = activeSessions.get(requestId)
    if (session) {
      session.unlistenChunk()
      session.unlistenDone()
      session.unlistenError?.()
      activeSessions.delete(requestId)
    }
  }

  function resetContent(): void {
    streamingContent.value = ''
  }

  function destroyAll(): void {
    for (const [, session] of activeSessions) {
      session.unlistenChunk()
      session.unlistenDone()
      session.unlistenError?.()
    }
    activeSessions.clear()
    isStreaming.value = false
    streamingContent.value = ''
  }

  onUnmounted(() => {
    destroyAll()
  })

  return {
    isStreaming,
    streamingContent,
    startStream,
    stopStream,
    resetContent,
    destroyAll,
  }
}

export function generateRequestId(): string {
  return Date.now().toString(36) + Math.random().toString(36).substr(2)
}
