import { ref } from 'vue'
import { defineExtension } from '@/runtime/extension-registry'
import VideoView from './View.vue'
import './locales'

/** 跨扩展投递的待处理视频路径列表（finder-ext 等经同页事件写入，View 消费后清空）。 */
export const pendingInputPaths = ref<string[]>([])

export default defineExtension({
  meta: {
    id: 'video',
    name: { 'zh-CN': '视频处理', en: 'Video Tools' },
    description: {
      'zh-CN': '压缩、格式转换与提取音频',
      en: 'Compress, convert format and extract audio',
    },
    icon: 'i-ri-video-line',
    order: 115,
    keywords: [
      'video',
      '视频',
      '压缩',
      '转码',
      '转换',
      'ffmpeg',
      'gif',
      'mp4',
      'webm',
      '音频',
      '提取',
      'compress',
      'convert',
    ],
  },
  disableSearchInput: true,
  windowHeight: 'auto',
  mainView: () => VideoView,
  setup: async () => {
    // 跨扩展同页投递：finder-ext 等经 window CustomEvent 同步写入待处理视频路径
    // （多选区全量带入）。同步是硬要求——投递先于跳转首帧渲染，View watch 立即消费。
    window.addEventListener('video-pending-input-path', (e) => {
      const detail = (e as CustomEvent<string[]>).detail
      pendingInputPaths.value = Array.isArray(detail) ? detail.filter(Boolean) : []
    })
  },
})
