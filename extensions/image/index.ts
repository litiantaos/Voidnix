import { ref } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { defineExtension } from '@/runtime/extension-registry'
import ImageView from './View.vue'
import './locales'

/** 跨扩展投递的待处理图片路径（finder-ext 等经事件总线写入，View 消费后清空）。 */
export const pendingInputPath = ref('')

export default defineExtension({
  meta: {
    id: 'image',
    name: { 'zh-CN': '图片处理', en: 'Image Tools' },
    description: { 'zh-CN': '移除背景、拼接长图', en: 'Remove background and stitch images' },
    icon: 'i-ri-image-edit-line',
    order: 116,
    keywords: [
      'image',
      '图片',
      '抠图',
      '背景',
      '去背景',
      'remove',
      'bg',
      'nobg',
      '透明',
      'matting',
      'segmentation',
      '拼接',
      '长图',
      'stitch',
      '合并',
      '拼图',
    ],
  },
  disableSearchInput: true,
  windowHeight: 'auto',
  mainView: () => ImageView,
  setup: async () => {
    // 跨扩展通信：finder-ext 等通过事件总线投递待处理图片路径
    await listen<string>('image-pending-input-path', (e) => {
      pendingInputPath.value = e.payload || ''
    })
  },
})
