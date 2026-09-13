import { ref } from 'vue'
import { defineExtension } from '@/runtime/extension-registry'
import { useAppStore } from '@/stores/app'
import './locales'
import ScreenshotView from './View.vue'
import ScreenshotOcr from './OcrView.vue'

// OCR 待识别数据（由截屏标注界面通过 open_extension_subview 触发时注入）
export const pendingOcrData = ref<{
  selX: number
  selY: number
  selW: number
  selH: number
  scale: number
  annotationPng: string
  previewPng: string
} | null>(null)

/// OCR 会话状态（模块级单例）：KeepAlive 缓存超限 LRU 驱逐 / navigate 重载都会
/// 销毁 OcrView 组件，局部状态随之丢失（卸载后重进内容空白）——提升到模块级，
/// 组件重挂载直接恢复现场；识别进行中隐藏亦然（invoke 回调写模块级 ref）。
export const ocrSession = ref({
  imageUrl: '',
  ocrText: '',
  error: '',
  loading: false,
})

export default defineExtension({
  meta: {
    id: 'screenshot',
    name: { 'zh-CN': '截屏', en: 'Screenshot' },
    description: { 'zh-CN': '截屏、标注、OCR', en: 'Screenshot, annotation, OCR' },
    icon: 'i-ri-screenshot-line',
    keywords: [
      'screenshot',
      '截屏',
      '截图',
      'jietu',
      'ocr',
      '识别',
      '文字识别',
      'shibie',
      'qr',
      '二维码',
      'erweima',
      'barcode',
    ],
    order: 110,
  },

  mainView: () => ScreenshotView,
  subviews: { ocr: () => ScreenshotOcr },
  subviewHeights: { ocr: 'auto' },
  disableSearchInput: true,
  globalShortcuts: [
    {
      id: 'screenshot',
      default: 'Alt+S',
      onExecute: () => {
        // Rust 端 hook 已在 shortcut.rs 中处理截屏全流程（capture + enter 模式），
        // 前端 onExecute 此处为占位，确保扩展声明让 App.vue 能注册快捷键。
      },
    },
  ],
  onOpenSubview: (_subviewId: string, payload: unknown) => {
    const d = payload as {
      selX: number
      selY: number
      selW: number
      selH: number
      scale: number
      annotationPng: string
      previewPng?: string
    }
    pendingOcrData.value = {
      selX: d.selX,
      selY: d.selY,
      selW: d.selW,
      selH: d.selH,
      scale: d.scale,
      annotationPng: d.annotationPng,
      previewPng: d.previewPng ?? '',
    }
  },
  onExecute: async (result) => {
    if (result.data?.openSubview) {
      const appStore = useAppStore()
      appStore.setActiveExtension('screenshot')
      appStore.setSearchQuery('')
      appStore.openSubview('ocr')
    }
  },
})
