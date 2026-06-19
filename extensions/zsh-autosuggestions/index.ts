import { defineExtension } from '@/runtime/extension-registry'
import { defineAsyncComponent } from 'vue'

const View = defineAsyncComponent(() => import('./View.vue'))

export default defineExtension({
  meta: {
    id: 'zsh-autosuggestions',
    name: '终端自动建议',
    description: 'zsh 命令行智能补全',
    icon: 'i-ri-terminal-box-line',
    keywords: ['zsh', 'autosuggestions', '终端', '命令', '补全', '预测', 'shell', '历史'],
    order: 80,
  },

  mainView: () => View,
})
