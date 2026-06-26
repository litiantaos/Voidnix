import { ref } from 'vue'
import { defineExtension } from '@/runtime/extension-registry'
import { defineAsyncComponent } from 'vue'
import ScreenshotView from './View.vue'
import ScreenshotOcr from './OcrView.vue'

// 独立窗口（截图标注 host / pin）真按需：低频重逻辑，不触发截图/pin 不加载，省稳态内存
const ScreenshotWindow = defineAsyncComponent(() => import('./windows/Host.vue'))
const PinWindow = defineAsyncComponent(() => import('./windows/PinWindow.vue'))

// OCR 待识别数据（由截屏标注界面通过 open_module_subview 触发时注入）
export const pendingOcrData = ref<{
  selX: number
  selY: number
  selW: number
  selH: number
  scale: number
  annotationPng: string
  previewPng: string
} | null>(null)

export default defineExtension({
  meta: {
    id: 'screenshot',
    name: '截屏',
    description: '截屏、标注、OCR',
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
    order: 11,
  },

  mainView: () => ScreenshotView,
  subviews: { ocr: () => ScreenshotOcr },
  windowViews: {
    screenshot: () => ScreenshotWindow,
    'pin-': () => PinWindow,
  },
  globalShortcuts: [
    {
      id: 'screenshot',
      default: 'Alt+S',
      onExecute: () => {
        // Rust 端 hook 已在 shortcut.rs 中处理截屏全流程（capture + enter 模式），
        // 前端 onExecute 此处为占位，确保模块声明让 App.vue 能注册快捷键。
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
      const { useAppStore } = await import('@/stores/app')
      const appStore = useAppStore()
      appStore.setActiveModule('screenshot')
      appStore.setSearchQuery('')
      appStore.openSubview('ocr')
    }
  },
})
