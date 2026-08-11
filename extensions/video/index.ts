import { ref } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { defineExtension } from '@/runtime/extension-registry'
import VideoView from './View.vue'
import './locales'

/** 跨扩展投递的待处理视频路径（finder-ext 等经事件总线写入，View 消费后清空）。 */
export const pendingInputPath = ref('')

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
    // 跨扩展通信：finder-ext 等通过事件总线投递待处理视频路径
    await listen<string>('video-pending-input-path', (e) => {
      pendingInputPath.value = e.payload || ''
    })
  },
})
