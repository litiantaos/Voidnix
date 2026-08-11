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
  'ai-providers.urlRequired': { 'zh-CN': '请填写 API URL', en: 'Please enter the API URL' },
  'ai-providers.keyRequired': { 'zh-CN': '请填写 API Key', en: 'Please enter the API Key' },
  'ai-providers.pasteFailed': { 'zh-CN': '粘贴失败', en: 'Paste failed' },
  'ai-providers.fieldEmpty': { 'zh-CN': '{name} 为空', en: '{name} is empty' },
  'ai-providers.configGuide': { 'zh-CN': '配置说明', en: 'Config Guide' },
  'ai-providers.usageGuide': { 'zh-CN': '使用说明', en: 'Usage Guide' },
  'ai-providers.helpMarkdown': {
    'zh-CN': `提供商配置保存后写入 \`~/.config/voidnix/ai.env\`，新开终端生效。

- Key 导出为 \`VOIDNIX_*_API_KEY\`，端点导出为 \`VOIDNIX_*_BASE_URL\`
- 智谱、DeepSeek 用固定后缀，如 \`VOIDNIX_ZHIPU_API_KEY\`，其余按名称推导
- 外部工具须显式引用，如 OpenCode \`{env:VOIDNIX_ZHIPU_API_KEY}\`
- 选中 Key 按下 **Cmd+Enter** 可粘贴 Key / 端点 / 模型名`,
    en: `Provider config is written to \`~/.config/voidnix/ai.env\` after saving; new terminals pick it up.

- Keys are exported as \`VOIDNIX_*_API_KEY\`, endpoints as \`VOIDNIX_*_BASE_URL\`
- Zhipu and DeepSeek use fixed suffixes, e.g. \`VOIDNIX_ZHIPU_API_KEY\`; others are derived from the name
- External tools must reference them explicitly, e.g. OpenCode \`{env:VOIDNIX_ZHIPU_API_KEY}\`
- Select a key and press **Cmd+Enter** to paste the key / endpoint / model name`,
  },
})
