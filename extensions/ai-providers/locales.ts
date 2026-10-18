import { registerMessages } from '@/runtime/i18n'

registerMessages({
  'ai-providers.empty': { 'zh-CN': '请添加 AI 提供商', en: 'Please add an AI provider' },
  'ai-providers.addProvider': { 'zh-CN': '添加提供商', en: 'Add Provider' },
  'ai-providers.editProvider': { 'zh-CN': '编辑提供商', en: 'Edit Provider' },
  'ai-providers.addKey': { 'zh-CN': '添加 Key', en: 'Add Key' },
  'ai-providers.editKey': { 'zh-CN': '编辑 Key', en: 'Edit Key' },
  'ai-providers.name': { 'zh-CN': '名称', en: 'Name' },
  'ai-providers.modelId': { 'zh-CN': '模型 ID', en: 'Model ID' },
  'ai-providers.modelLabel': { 'zh-CN': '模型', en: 'Model' },
  'ai-providers.label': { 'zh-CN': '备注', en: 'Note' },
  'ai-providers.labelPlaceholder': { 'zh-CN': '主号 / 备用', en: 'Primary / Backup' },
  'ai-providers.unnamedProvider': { 'zh-CN': '未命名提供商', en: 'Unnamed Provider' },
  'ai-providers.insufficientBalance': { 'zh-CN': '余额不足', en: 'Insufficient balance' },
  'ai-providers.noKey': { 'zh-CN': '无 Key', en: 'No Key' },
  'ai-providers.available': { 'zh-CN': '可用', en: 'Available' },
  'ai-providers.loadingUsage': { 'zh-CN': '获取用量信息中…', en: 'Loading usage…' },
  'ai-providers.paste': { 'zh-CN': '粘贴 {name}', en: 'Paste {name}' },
  'ai-providers.deleteKey': { 'zh-CN': '删除 Key', en: 'Delete Key' },
  'ai-providers.keyDeleted': { 'zh-CN': '已删除 Key', en: 'Key deleted' },
  'ai-providers.default': { 'zh-CN': '默认', en: 'Default' },
  'ai-providers.urlRequired': {
    'zh-CN': '请填写 OpenAI Chat URL',
    en: 'Please enter the OpenAI Chat URL',
  },
  'ai-providers.keyRequired': { 'zh-CN': '请填写 API Key', en: 'Please enter the API Key' },
  'ai-providers.pasteFailed': { 'zh-CN': '粘贴失败', en: 'Paste failed' },
  'ai-providers.fieldEmpty': { 'zh-CN': '{name} 为空', en: '{name} is empty' },
  'ai-providers.configGuide': { 'zh-CN': '配置说明', en: 'Config Guide' },
  'ai-providers.usageGuide': { 'zh-CN': '使用说明', en: 'Usage Guide' },
  'ai-providers.helpMarkdown': {
    'zh-CN': `统一维护提供商的三种协议端点（OpenAI Chat / OpenAI Responses / Anthropic Messages）/ Key / 模型，供应用内扩展（Agent、翻译）与 AI 网关消费。

- 外部工具（Claude Code、OpenAI 兼容 CLI 等）经 **AI 网关** 接入：把 API 地址指向 \`http://127.0.0.1:8788\`、Key 随便填，网关按请求模型路由并注入真实 Key（多 Key 自动轮换）
- Anthropic 端点（可选）供网关直通 Claude Code 等 Anthropic 客户端（智谱 \`https://open.bigmodel.cn/api/anthropic\`、DeepSeek \`https://api.deepseek.com/anthropic\`），声明后模型进入网关路由
- 选中 Key 按下 **Cmd+Enter** 可粘贴 Key / URL / 模型名（声明了 Responses 端点时多一条粘贴项）`,
    en: `A single place for provider endpoints of the three wire protocols (OpenAI Chat / OpenAI Responses / Anthropic Messages), keys and models — consumed by in-app extensions (Agent, Translate) and the AI gateway.

- External tools (Claude Code, OpenAI-compatible CLIs, …) connect through the **AI gateway**: point the API base URL at \`http://127.0.0.1:8788\` with any placeholder key; the gateway routes by model and injects real keys (with rotation)
- The optional Anthropic endpoint lets the gateway serve Claude Code and other Anthropic clients (Zhipu \`https://open.bigmodel.cn/api/anthropic\`, DeepSeek \`https://api.deepseek.com/anthropic\`); declared models join gateway routing
- Select a key and press **Cmd+Enter** to paste the key / endpoint / model name (an extra item appears when a Responses endpoint is declared)`,
  },
})
