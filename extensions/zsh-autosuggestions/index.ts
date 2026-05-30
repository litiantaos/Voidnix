import { registerModule } from '@/core/module-registry'
import { asyncView } from '@/core/async-view'
import { keywordModuleSearch } from '@/core/module-helpers'
import type { AppModule } from '@/types/module'

const View = asyncView(() => import('./View.vue'))

const keywords = ['zsh', 'autosuggestions', '终端', '命令', '补全', '预测', 'shell', '历史']

const mod: AppModule = {
  id: 'zsh-autosuggestions',
  name: '终端自动建议',
  description: 'zsh 命令行智能预测补全，根据历史记录、使用频率和目录上下文提供 ghost text 建议',
  icon: 'i-ri-terminal-box-line',
  keywords,
  order: 80,
  layout: { view: View },
  onSearch: async (query) => keywordModuleSearch(mod, query),
}

registerModule(mod)
