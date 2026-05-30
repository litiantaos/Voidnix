import { ref } from 'vue'
import { registerModule } from '@/core/module-registry'
import { asyncView } from '@/core/async-view'
import { moduleSelfResult } from '@/core/module-helpers'
import type { AppModule } from '@/types/module'

const ScreenshotView = asyncView(() => import('./View.vue'))
const ScreenshotWindow = asyncView(() => import('./windows/Host.vue'))
const PinWindow = asyncView(() => import('./windows/PinWindow.vue'))
const ScreenshotOcr = asyncView(() => import('./OcrView.vue'))

// OCR 待识别数据（由截屏标注界面通过 open_module_panel 触发时注入）
export const pendingOcrData = ref<{
  selX: number
  selY: number
  selW: number
  selH: number
  scale: number
  annotationPng: string
  previewPng: string
} | null>(null)

const mod: AppModule = {
  id: 'screenshot',
  name: '截屏',
  description: '区域截屏、标注、OCR',
  icon: 'i-ri-screenshot-line',
  keywords: ['screenshot', '截屏', '截图', 'jietu', 'ocr'],
  order: 9,
  layout: { view: ScreenshotView },
  panel: ScreenshotOcr,
  windowViews: {
    screenshot: ScreenshotWindow,
    'pin-': PinWindow,
  },
  globalShortcuts: [
    {
      id: 'screenshot',
      default: 'CommandOrControl+Shift+X',
      onExecute: () => {
        // Rust 端 hook 已在 shortcut.rs 中处理截屏全流程（capture + enter 模式），
        // 前端 onExecute 此处为占位，确保模块声明让 App.vue 能注册快捷键。
      },
    },
  ],
  onOpenPanel: (payload: unknown) => {
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
  onSearch: async (query) => {
    if (!query.trim()) return []
    if (
      'ocr'.includes(query.toLowerCase()) ||
      '识别'.includes(query) ||
      '文字识别'.includes(query) ||
      'shibie'.includes(query.toLowerCase())
    ) {
      return [moduleSelfResult(mod)]
    }
    return []
  },
  onExecute: async (result) => {
    if (result.data?.openPanel) {
      const { useAppStore } = await import('@/stores/app')
      const appStore = useAppStore()
      appStore.setActiveModule('screenshot')
      appStore.setSearchQuery('')
      appStore.showPanel = true
    }
  },
}

registerModule(mod)