import { defineExtension } from '@/runtime/extension-registry'
import { makeToggleHandler } from '@/stores/app'
import AgentSettings from './Settings.vue'
import AgentView from './View.vue'
import AgentActions from './Actions.vue'
import './locales'

export default defineExtension({
  meta: {
    id: 'agent',
    name: 'AI Agent',
    description: {
      'zh-CN': '支持对话和操作的人工智能助手',
      en: 'AI assistant with chat and tool-calling',
    },
    icon: 'i-ri-chat-ai-line',
    keywords: ['agent', 'ai', 'gpt', '对话', '聊天', '助手', 'assistant', '搜索'],
    order: 30,
  },

  disableSearchInput: true,
  mainView: () => AgentView,
  searchBarAccessory: () => AgentActions,
  subviews: { config: () => AgentSettings },
  windowHeight: 840,
  globalShortcuts: [
    {
      id: 'agent',
      default: 'Alt+A',
      onExecute: makeToggleHandler('agent'),
    },
  ],
})
