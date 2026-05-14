import { createApp } from 'vue'
import { createPinia } from 'pinia'
import { initAllModules } from '@/core/module-registry'
import App from './App.vue'
import 'virtual:uno.css'
import './styles/main.css'

// 自动发现并注册所有模块 (放置在最外层以解决循环依赖)
import.meta.glob('@/modules/*/index.ts', { eager: true })

const app = createApp(App)
app.use(createPinia())
app.mount('#app')

// 异步初始化模块，不阻塞 Vue 挂载和全局快捷键注册
initAllModules().catch(e => {
  console.error('Failed to init modules:', e)
})
