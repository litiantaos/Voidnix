import { defineExtension } from '@/runtime/extension-registry'
import './locales'
import AiProvidersView from './View.vue'
import AiProvidersActions from './Actions.vue'
import { setupAiProvidersSync } from './sync'

export default defineExtension({
  meta: {
    id: 'ai-providers',
    name: { 'zh-CN': 'AI 提供商', en: 'AI Providers' },
    description: {
      'zh-CN': '统一管理 AI API Key，供本应用与外部工具共用',
      en: 'Unified AI API key management for app and external tools',
    },
    icon: 'i-ri-key-2-line',
    keywords: [
      'ai',
      'provider',
      'api',
      'key',
      'openai',
      'llm',
      '模型',
      '密钥',
      '提供商',
      'opencode',
      'grok',
    ],
    order: 35,
  },

  disableSearchInput: true,
  mainView: () => AiProvidersView,
  searchBarAccessory: () => AiProvidersActions,

  async setup() {
    await setupAiProvidersSync()
  },
})
