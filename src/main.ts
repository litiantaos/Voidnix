import { createApp } from 'vue'
import { createPinia } from 'pinia'
import { getAllExtensions } from '@/runtime/extension-registry'
import App from './App.vue'
import 'virtual:uno.css'
import './styles/theme.css'

// 自动发现并注册所有扩展：各 index.ts 顶层调 defineExtension({...}) 完成注册
import.meta.glob(['@ext/*/index.ts'], { eager: true })

const app = createApp(App)
app.use(createPinia())
app.mount('#app')

// 异步执行扩展 setup 钩子（不阻塞 Vue 挂载与全局快捷键注册）
// 每个扩展独立 try/catch：单个故障不拖垮其他扩展的初始化（扩展自治）
Promise.all(
  getAllExtensions().map(async (e) => {
    try {
      await e.setup?.()
    } catch (err) {
      console.error(`[setup:${e.meta.id}] failed:`, err)
    }
  }),
)
