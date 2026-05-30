import { onMounted, onUnmounted } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

export function useTauriListener<T = unknown>(event: string, handler: (payload: T) => void) {
  let unlisten: UnlistenFn | undefined

  onMounted(async () => {
    unlisten = await listen<T>(event, (e) => handler(e.payload))
  })

  onUnmounted(() => {
    unlisten?.()
  })
}
