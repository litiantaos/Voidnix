import { registerMessages } from '@/runtime/i18n'

registerMessages({
  // ─── View ──────────────────────────────────
  'agent.setupTitle': {
    'zh-CN': '请先配置 AI 提供商',
    en: 'Please configure an AI provider first',
  },
  'agent.emptyTitle': { 'zh-CN': '来点有意思的吧！', en: "Let's do something fun!" },
  'agent.emptyHint': {
    'zh-CN': '日常问题、工作任务、搜索资料、跑命令...',
    en: 'Daily questions, work tasks, research, run commands…',
  },
  'agent.scrollBottom': { 'zh-CN': '滚到底部', en: 'Scroll to bottom' },
  'agent.stopShortcut': { 'zh-CN': '中止（Ctrl+C）', en: 'Stop (Ctrl+C)' },
  'agent.stop': { 'zh-CN': '中止生成', en: 'Stop generating' },
  'agent.placeholder': { 'zh-CN': '聊点什么...', en: 'Chat about anything…' },

  // ─── 流式 step ──────────────────────────────
  'agent.thinking': { 'zh-CN': '思考', en: 'Thinking' },
  'agent.copyFailed': { 'zh-CN': '复制失败', en: 'Copy failed' },

  // ─── 工具语义 label ─────────────────────────
  'agent.tool.search': { 'zh-CN': '搜索', en: 'Search' },
  'agent.tool.command': { 'zh-CN': '命令', en: 'Command' },
  'agent.tool.default': { 'zh-CN': '工具', en: 'Tool' },

  // ─── Settings ───────────────────────────────
  'agent.editSearchProvider': { 'zh-CN': '编辑搜索提供商', en: 'Edit Search Provider' },
  'agent.systemPrompt': { 'zh-CN': '系统提示词', en: 'System Prompt' },
  'agent.systemPromptPlaceholder': {
    'zh-CN': '定义 Agent 角色、能力边界、工具使用规则、安全约束与输出风格',
    en: 'Define Agent role, capabilities, tool rules, safety constraints, and output style',
  },
  'agent.reset': { 'zh-CN': '重置', en: 'Reset' },
  'agent.notSet': { 'zh-CN': '未设置', en: 'Not set' },
  'agent.configureInAiProviders': {
    'zh-CN': '在「AI 提供商」中配置',
    en: 'Configure in "AI Providers"',
  },
  'agent.configured': { 'zh-CN': '已配置', en: 'Configured' },
  'agent.notConfigured': { 'zh-CN': '未配置', en: 'Not configured' },
  'agent.group.provider': { 'zh-CN': '提供商', en: 'Provider' },
  'agent.group.advanced': { 'zh-CN': '高级', en: 'Advanced' },

  // ─── agent.ts notice messages ───────────────
  'agent.noProviderConfigured': {
    'zh-CN': '请先在「AI 提供商」配置提供商（endpoint / API Key / 模型）。',
    en: 'Please configure a provider (endpoint / API Key / model) in "AI Providers" first.',
  },
  'agent.aborted': { 'zh-CN': '已中止', en: 'Aborted' },

  // ─── Actions.vue ────────────────────────────
  'agent.history': { 'zh-CN': '历史消息', en: 'History' },
  'agent.newChat': { 'zh-CN': '新会话', en: 'New chat' },
  'agent.settings': { 'zh-CN': '设置', en: 'Settings' },
  'agent.closeSettings': { 'zh-CN': '关闭设置', en: 'Close settings' },
})
