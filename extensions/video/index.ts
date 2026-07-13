import { defineExtension } from '@/runtime/extension-registry'
import VideoView from './View.vue'

export default defineExtension({
  meta: {
    id: 'video',
    name: '视频处理',
    description: '压缩、格式转换与提取音频',
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
})
