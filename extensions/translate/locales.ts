import { registerMessages } from '@/runtime/i18n'

registerMessages({
  // ─── View ──────────────────────────────────
  'translate.notConfigured': {
    'zh-CN': '请先配置翻译服务',
    en: 'Please configure translation service first',
  },
  'translate.inputPlaceholder': { 'zh-CN': '输入文本', en: 'Enter text' },
  'translate.speak': { 'zh-CN': '朗读', en: 'Speak' },
  'translate.speakFailed': { 'zh-CN': '朗读失败', en: 'Speech failed' },

  // ─── Settings ──────────────────────────────
  'translate.youdao': { 'zh-CN': '有道翻译', en: 'Youdao Translate' },
  'translate.ai': { 'zh-CN': 'AI 翻译', en: 'AI Translation' },
  'translate.manageProviders': { 'zh-CN': '管理提供商', en: 'Manage Providers' },
  'translate.openAiProviders': { 'zh-CN': '打开 AI 提供商', en: 'Open AI Providers' },
  'translate.selectModel': { 'zh-CN': '选择翻译模型', en: 'Select Translation Models' },
  'translate.prompt': { 'zh-CN': '提示词', en: 'Prompt' },
  'translate.configured': { 'zh-CN': '已配置', en: 'Configured' },
  'translate.notConfiguredStatus': { 'zh-CN': '未配置', en: 'Not Configured' },
  'translate.noModelSelected': { 'zh-CN': '未选择模型', en: 'No model selected' },
  'translate.listSeparator': { 'zh-CN': '、', en: ', ' },
  'translate.andMore': { 'zh-CN': ' 等 {count} 个', en: ' +{count} more' },
  'translate.modelAndKey': { 'zh-CN': '模型与 Key', en: 'Model & Key' },
  'translate.model': { 'zh-CN': '模型', en: 'Model' },
  'translate.targetLanguage': { 'zh-CN': '目标语言', en: 'Target Language' },
  'translate.group.service': { 'zh-CN': '翻译服务', en: 'Translation Service' },

  // ─── 目标语言 ───────────────────────────────
  'translate.lang.zh': { 'zh-CN': '中文', en: 'Chinese' },
  'translate.lang.en': { 'zh-CN': '英文', en: 'English' },
  'translate.lang.ja': { 'zh-CN': '日文', en: 'Japanese' },
  'translate.lang.ko': { 'zh-CN': '韩文', en: 'Korean' },
  'translate.lang.fr': { 'zh-CN': '法文', en: 'French' },
  'translate.lang.de': { 'zh-CN': '德文', en: 'German' },
  'translate.lang.es': { 'zh-CN': '西班牙文', en: 'Spanish' },

  // ─── config 语义 ───────────────────────────
  'translate.envVars': { 'zh-CN': '环境变量', en: 'Environment Variables' },
})
