import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { defineExtension } from '@/runtime/extension-registry'
import { CMD } from '@/commands'
import { showToast } from '@/composables/useToast'
import { t } from '@/runtime/i18n'
import VideoView from './View.vue'
import './locales'

/** 跨扩展投递的待处理视频路径列表（finder-ext 等经同页事件写入，View 消费后清空）。 */
export const pendingInputPaths = ref<string[]>([])

export default defineExtension({
  meta: {
    id: 'video',
    name: { 'zh-CN': '视频处理', en: 'Video Tools' },
    description: { 'zh-CN': '压缩、转格式、提取音频', en: 'Compress, convert, extract audio' },
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
  mainView: VideoView,
  setup: async () => {
    // 跨扩展同页投递：finder-ext 等经 window CustomEvent 同步写入待处理视频路径
    // （多选区全量带入）。同步是硬要求——投递先于跳转首帧渲染，View watch 立即消费。
    window.addEventListener('video-pending-input-path', (e) => {
      const detail = (e as CustomEvent<string[]>).detail
      pendingInputPaths.value = Array.isArray(detail) ? detail.filter(Boolean) : []
    })

    // 录屏闭环交接（screenshot 扩展录屏结束后经同名 CustomEvent 投递录制文件）：
    // 直接进独立剪辑台，不再经 video 面板中转。与面板「剪辑」入口同序——先隐藏
    // 主窗（主窗此刻通常已隐藏，hide 幂等），再创建剪辑窗口接管键盘。
    window.addEventListener('video-clip-open', (e) => {
      const detail = (e as CustomEvent<string[]>).detail
      const paths = Array.isArray(detail) ? detail.filter(Boolean) : []
      if (!paths.length) return
      void (async () => {
        try {
          // 剪辑窗 bootstrap 依赖 ffprobe：核心不可用时先提示，不开一个必然报错的窗口
          const core = await invoke<{ available: boolean }>(CMD.videoCoreStatus).catch(() => null)
          if (!core?.available) {
            void showToast(t('video.clip.needCore'), { kind: 'error', duration: 5000 })
            return
          }
          await invoke(CMD.hideWindow)
          await invoke(CMD.videoClipOpen, { paths })
        } catch (err) {
          console.error('video-clip-open failed:', err)
          void showToast(String(err), { kind: 'error', duration: 4000 })
        }
      })()
    })
  },
})
