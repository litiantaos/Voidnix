import { ref } from 'vue'
import { defineExtension } from '@/runtime/extension-registry'
import ImageView from './View.vue'
import './locales'

/** 跨扩展投递的待处理图片路径列表（finder-ext 等经同页事件写入，View 消费后清空）。 */
export const pendingInputPaths = ref<string[]>([])

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
    // 跨扩展同页投递：finder-ext 等经 window CustomEvent 同步写入待处理图片路径
    // （多选区全量带入）。同步是硬要求——投递必须先于跳转首帧渲染（列表形状一次到位），异步到达期间
    // 用户 ↓+Enter 会击中「source（选择文件）」行误开系统文件选择器。
    window.addEventListener('image-pending-input-path', (e) => {
      const detail = (e as CustomEvent<string[]>).detail
      pendingInputPaths.value = Array.isArray(detail) ? detail.filter(Boolean) : []
    })
  },
})
