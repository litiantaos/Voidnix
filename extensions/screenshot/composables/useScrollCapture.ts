import { ref, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { Sel } from './useTypes'

/// 滚动截屏：进入后 Rust 装挖洞遮罩 + 启动抓帧线程，预览帧通过事件推。
/// 三态：idle → running → finishing/finished/cancelled。
/// 完成后 result 持有最终 PNG dataURL（前端用于保存/复制）。
export function useScrollCapture(options: { dpr: { value: number } }) {
  const isActive = ref(false)
  const previewDataUrl = ref<string>('')
  const previewWidth = ref(0)
  const previewHeight = ref(0)
  const result = ref<string>('')
  const error = ref<string | null>(null)
  const isFinishing = ref(false)
  /// 鼠标当前是否在选区"洞"内（事件穿透状态）。仅供 UI 反馈。
  const isPassthrough = ref(false)

  let unlistenFrame: UnlistenFn | null = null
  let unlistenPassthrough: UnlistenFn | null = null

  async function start(sel: Sel) {
    if (isActive.value) return
    error.value = null
    previewDataUrl.value = ''
    previewWidth.value = 0
    previewHeight.value = 0
    result.value = ''

    // 监听帧事件
    unlistenFrame = await listen<{
      seq: number
      width: number
      height: number
      dataUrl: string
    }>('screenshot-scroll-frame', (event) => {
      previewDataUrl.value = event.payload.dataUrl
      previewWidth.value = event.payload.width
      previewHeight.value = event.payload.height
    })

    // 监听 passthrough 状态切换
    unlistenPassthrough = await listen<boolean>(
      'screenshot-scroll-passthrough',
      (event) => {
        isPassthrough.value = event.payload
      },
    )

    try {
      await invoke('enter_scroll_capture', {
        selX: sel.x,
        selY: sel.y,
        selW: sel.w,
        selH: sel.h,
        scale: options.dpr.value,
      })
      isActive.value = true
    } catch (e) {
      error.value = String(e)
      if (unlistenFrame) {
        unlistenFrame()
        unlistenFrame = null
      }
      if (unlistenPassthrough) {
        unlistenPassthrough()
        unlistenPassthrough = null
      }
    }
  }

  /// 完成：停止抓帧、获取最终 PNG。返回 dataURL（也存到 result 里）。
  async function finish(): Promise<string> {
    if (!isActive.value) return ''
    isFinishing.value = true
    try {
      const dataUrl = await invoke<string>('finish_scroll_capture')
      result.value = dataUrl
      isActive.value = false
      if (unlistenFrame) {
        unlistenFrame()
        unlistenFrame = null
      }
      if (unlistenPassthrough) {
        unlistenPassthrough()
        unlistenPassthrough = null
      }
      return dataUrl
    } catch (e) {
      error.value = String(e)
      throw e
    } finally {
      isFinishing.value = false
    }
  }

  /// 取消：停止抓帧、丢弃缓冲。
  async function cancel() {
    if (!isActive.value && !isFinishing.value) return
    try {
      await invoke('exit_scroll_capture')
    } catch (e) {
      console.error('[scroll-capture] exit failed:', e)
    }
    isActive.value = false
    isFinishing.value = false
    previewDataUrl.value = ''
    if (unlistenFrame) {
      unlistenFrame()
      unlistenFrame = null
    }
    if (unlistenPassthrough) {
      unlistenPassthrough()
      unlistenPassthrough = null
    }
  }

  onUnmounted(() => {
    if (isActive.value) {
      // 兜底：组件销毁时强制结束滚动会话
      invoke('exit_scroll_capture').catch(() => {})
    }
    if (unlistenFrame) {
      unlistenFrame()
      unlistenFrame = null
    }
    if (unlistenPassthrough) {
      unlistenPassthrough()
      unlistenPassthrough = null
    }
  })

  return {
    isActive,
    isFinishing,
    isPassthrough,
    previewDataUrl,
    previewWidth,
    previewHeight,
    result,
    error,
    start,
    finish,
    cancel,
  }
}
