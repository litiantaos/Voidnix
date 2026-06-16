import { asyncView } from '@/core/async-view'
import { registerModule } from '@/core/module-registry'
import { makeToggleHandler } from '@/core/module-helpers'
import type { AppModule } from '@/types/module'

const AgentView = asyncView(() => import('./View.vue'))
const AgentSettings = asyncView(() => import('./Settings.vue'))
const AgentActions = asyncView(() => import('./Actions.vue'))

const mod: AppModule = {
  id: 'agent',
  name: 'AI Agent',
  description: 'AI 助手（对话 + 网络搜索 + 命令执行）',
  icon: 'i-ri-chat-ai-line',
  keywords: ['agent', 'ai', 'gpt', '对话', '聊天', '助手', 'assistant', '搜索'],
  order: 9,
  disableSearchInput: true,
  view: AgentView,
  searchBarAccessory: AgentActions,
  subviews: { settings: AgentSettings },
  globalShortcuts: [
    {
      id: 'agent',
      default: 'CommandOrControl+Shift+A',
      onExecute: makeToggleHandler('agent'),
    },
  ],
  onInit: async () => {
    // useAgentChat 是 composable，事件流通过 Channel 自管理，无需全局 listener
  },
  onSearch: async () => [],
  onModuleSearch: async () => [],
  onExecute: async () => {},
}

registerModule(mod)
