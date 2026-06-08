import { createApp } from 'vue'
import { createPinia } from 'pinia'
import { initAllModules } from '@/core/module-registry'
import { loadTier2Extensions } from '@/core/tier2-registry'
import { preloadAllViews } from '@/core/async-view'
import App from './App.vue'
import 'virtual:uno.css'
import './styles/theme.css'

// 自动发现并注册所有 Tier 1 扩展 (放置在最外层以解决循环依赖)
// 所有扩展在 extensions/<name>/index.ts 下，由 Vite alias @ext 解析
import.meta.glob(['@ext/*/index.ts'], { eager: true })

const app = createApp(App)
app.use(createPinia())
app.mount('#app')

// 异步初始化 Tier 1 模块，不阻塞 Vue 挂载和全局快捷键注册
initAllModules().catch((e) => {
  console.error('Failed to init modules:', e)
})

// 加载 Tier 2 第三方扩展
loadTier2Extensions().catch((e) => {
  console.error('Failed to load Tier 2 extensions:', e)
})

// 并发预热所有扩展视图 chunk，消除首次激活时的拉取卡顿
preloadAllViews()
