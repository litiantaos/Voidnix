import { defineExtension } from '@/runtime/extension-registry'
import './locales'
import AiGatewayView from './View.vue'
import AiGatewayActions from './Actions.vue'
import { setupAiGatewaySync } from './sync'

export default defineExtension({
  meta: {
    id: 'ai-gateway',
    name: { 'zh-CN': 'AI 网关', en: 'AI Gateway' },
    description: {
      'zh-CN': '所有 AI 工具，共用一个入口',
      en: 'One local entry point for all your AI tools',
    },
    icon: 'i-ri-shuffle-line',
    keywords: [
      'gateway',
      'claude',
      'code',
      'anthropic',
      'responses',
      'rotate',
      'openai',
      '网关',
      '轮换',
      '切换',
    ],
    order: 36,
  },

  disableSearchInput: true,
  windowHeight: 'auto',
  mainView: () => AiGatewayView,
  searchBarAccessory: () => AiGatewayActions,

  async setup() {
    await setupAiGatewaySync()
  },
})
