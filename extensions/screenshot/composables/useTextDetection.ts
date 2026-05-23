import { ref, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { TextRegion } from './useTypes'

/// 懒触发的文本区域检测：首次调用时启动 Vision 后端，结果缓存于内存。
/// 检测覆盖整张截屏，前端按形状位置自行过滤。
export function useTextDetection(options: { dpr: Ref<number> }) {
  const textRegions = ref<TextRegion[]>([])
  const isDetecting = ref(false)
  const hasDetected = ref(false)
  const detectError = ref<string | null>(null)

  async function detect(): Promise<void> {
    if (hasDetected.value || isDetecting.value) return
    isDetecting.value = true
    detectError.value = null
    try {
      const regions = await invoke<TextRegion[]>('detect_text_regions', {
        scale: options.dpr.value,
      })
      textRegions.value = regions
      hasDetected.value = true
    } catch (e) {
      detectError.value = String(e)
      hasDetected.value = true
    } finally {
      isDetecting.value = false
    }
  }

  return { textRegions, isDetecting, hasDetected, detectError, detect }
}
