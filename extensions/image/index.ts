import { ref } from 'vue'
import { defineExtension } from '@/runtime/extension-registry'
import ImageView from './View.vue'
import ImageActions from './Actions.vue'

/** 拼接文件列表（mainView 内共享）。 */
export const stitchFiles = ref<string[]>([])

/** 当前工具（Actions 搜索栏选择器与 View 共享）。 */
export type Tool = 'removeBg' | 'stitch'
export const tool = ref<Tool>('removeBg')

export default defineExtension({
  meta: {
    id: 'image',
    name: '图片处理',
    description: '移除背景、拼接长图',
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
  searchBarAccessory: () => ImageActions,
})
