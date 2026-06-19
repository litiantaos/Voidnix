import { defineExtension } from '@/runtime/extension-registry'
import { defineAsyncComponent } from 'vue'
import { makeToggleHandler } from '@/utils/module-toggle'

const AgentView = defineAsyncComponent(() => import('./View.vue'))
const AgentSettings = defineAsyncComponent(() => import('./Settings.vue'))
const AgentActions = defineAsyncComponent(() => import('./Actions.vue'))

export default defineExtension({
  meta: {
    id: 'agent',
    name: 'AI Agent',
    description: 'AI 助手（对话 + 网络搜索 + 命令执行）',
    icon: 'i-ri-chat-ai-line',
    keywords: ['agent', 'ai', 'gpt', '对话', '聊天', '助手', 'assistant', '搜索'],
    order: 9,
  },

  disableSearchInput: true,
  mainView: () => AgentView,
  searchBarAccessory: () => AgentActions,
  subviews: { settings: () => AgentSettings },
  globalShortcuts: [
    {
      id: 'agent',
      default: 'CommandOrControl+Shift+A',
      onExecute: makeToggleHandler('agent'),
    },
  ],
})
