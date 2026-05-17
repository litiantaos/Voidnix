import { ref } from 'vue'
import { defineAsyncComponent } from 'vue'
import { registerModule } from '@/core/module-registry'
import type { AppModule } from '@/types/module'

const OcrView = defineAsyncComponent(() => import('./View.vue'))

// 待识别的图片数据（由截屏模块触发时注入）
export const pendingOcrImagePath = ref('')
export const pendingOcrData = ref<{
  selX: number
  selY: number
  selW: number
  selH: number
  scale: number
  annotationPng: string
} | null>(null)

const mod: AppModule = {
  id: 'ocr',
  name: 'OCR',
  description: '截图文字识别',
  icon: 'i-ri-scan-line',
  keywords: ['ocr', '识别', '文字识别', 'shibie'],
  order: 10,
  hidden: true, // 不在搜索结果中出现，只从截屏触发
  useSearchInput: false, // 禁用主搜索框
  layout: { view: OcrView },
  multiline: true,
}

registerModule(mod)
