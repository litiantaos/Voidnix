import { defineAsyncComponent } from 'vue'
import { registerModule } from '@/core/module-registry'
import type { AppModule } from '@/types/module'

const ScreenshotView = defineAsyncComponent(() => import('./View.vue'))

const mod: AppModule = {
  id: 'screenshot',
  name: '截屏',
  description: '区域截屏、标注、OCR',
  icon: 'i-ri-screenshot-line',
  keywords: ['screenshot', '截屏', '截图', 'jietu', 'ocr'],
  order: 9,
  layout: { view: ScreenshotView },
}

registerModule(mod)
