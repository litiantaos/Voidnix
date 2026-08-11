import { createApp } from 'vue'
import { createPinia } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import { getAllExtensions } from '@/runtime/extension-registry'
import { initTheme } from '@/runtime/theme'
import { initI18n } from '@/runtime/i18n'
import { prewarmPinyin } from '@/utils/fuzzy'
import { useSystemStore } from '@/stores/system'
import { CMD } from '@/commands'
import { isTauri } from '@/utils/tauri'
import App from './App.vue'
import 'virtual:uno.css'
import './styles/theme.css'
import './styles/markdown.css'

// 注册框架级文案
import '@/locales'

// 自动发现并注册所有扩展：各 index.ts 顶层调 defineExtension({...}) 完成注册
import.meta.glob(['@ext/*/index.ts'], { eager: true })

const app = createApp(App)
app.use(createPinia())
app.mount('#app')

// 后台预热拼音模块：非阻塞，用户首次搜索中文前加载完成（不延迟首屏）
prewarmPinyin()

// 主题：尽早 apply（窗口默认隐藏，呼出前已是正确主题，无可见闪烁）
initTheme()

// i18n：尽早初始化（扩展 setup 前完成，扩展 setup 可能读文案）
initI18n()

// 后台预查系统状态（权限 + 开机自启）：设置页只读缓存值，消除首帧「检查中…」跳变。
// fire-and-forget 不阻塞启动；Rust 侧同步纳秒/微秒级，用户进设置页前早已就绪。
useSystemStore().refresh()

// 全局禁用 WKWebView 原生右键菜单（应用无任何右键交互场景，
// 输入框复制粘贴走 Cmd+C/V/A 原生快捷键；多窗口共享同一 bundle 一处生效）
document.addEventListener('contextmenu', (e) => e.preventDefault())

// 异步执行扩展 setup 钩子（不阻塞 Vue 挂载与全局快捷键注册）
// 每个扩展独立 try/catch：单个故障不拖垮其他扩展的初始化（扩展自治）
const setupDone = Promise.all(
  getAllExtensions().map(async (e) => {
    try {
      await e.setup?.()
    } catch (err) {
      console.error(`[setup:${e.meta.id}] failed:`, err)
    }
  }),
)

// 自测模式：扩展 setup 完成后触发（环境变量 VOIDNIX_SELF_TEST=1 驱动）。
// 加 10s 超时保护——某个扩展 setup 可能因网络/二进制下载阻塞（如 video 等 ffmpeg），
// 超时后仍触发自测（该扩展的视图冒烟会跳过/报错，不影响其余用例）。
// 动态 import 避免自测代码进入生产 bundle 的初始 chunk
if (isTauri) {
  invoke<boolean>(CMD.isSelfTestMode)
    .catch(() => false)
    .then(async (selfTest) => {
      if (!selfTest) return
      await Promise.race([setupDone, new Promise<void>((r) => setTimeout(r, 10000))])
      const { runSelfTest } = await import('./runtime/self-test')
      await runSelfTest()
    })
}
