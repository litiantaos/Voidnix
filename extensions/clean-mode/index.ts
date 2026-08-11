import { defineExtension } from '@/runtime/extension-registry'
import './locales'
import CleanModeView from './View.vue'

export default defineExtension({
  meta: {
    id: 'clean-mode',
    name: { 'zh-CN': '清洁模式', en: 'Clean Mode' },
    description: {
      'zh-CN': '黑屏并锁定键鼠，方便清洁屏幕和键盘',
      en: 'Black screen and lock input for cleaning',
    },
    icon: 'i-ri-contrast-2-fill',
    keywords: ['clean', '清洁', '清洗', '亮度', '锁定', 'lock', '黑屏', '屏幕清洁', '键盘清洁'],
    order: 150,
  },

  disableSearchInput: true,
  mainView: () => CleanModeView,
})
