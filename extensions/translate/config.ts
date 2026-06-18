import { defineConfig } from '@/runtime/storage'

/// translate 扩展自管配置（持久化至 extensions/translate/config.json）。
/// 注：translateConfigs（API 端点数组）暂留 settings.ts，因与 AI provider 基础设施共享管理逻辑。
export const config = defineConfig('translate', {
  targetLang: 'zh',
})
