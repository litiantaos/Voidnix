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
      'zh-CN': '统一本地 AI 网关：任何工具指向它，即得多提供商路由与 Key 轮换',
      en: 'Unified local AI gateway: point any tool at it for multi-provider routing and key rotation',
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
