import { defineExtension } from '@/runtime/extension-registry'
import './locales'
import View from './View.vue'

export default defineExtension({
  meta: {
    id: 'zsh-autosuggestions',
    name: { 'zh-CN': '终端自动建议', en: 'Terminal Autosuggestions' },
    description: { 'zh-CN': 'zsh 命令行智能补全', en: 'zsh command-line smart completion' },
    icon: 'i-ri-terminal-box-line',
    keywords: ['zsh', 'autosuggestions', '终端', '命令', '补全', '预测', 'shell', '历史'],
    order: 140,
  },

  disableSearchInput: true,
  mainView: () => View,
})
