/// 剪辑窗口独立入口：只加载 Vue + ClipWindow 组件 + 主题 CSS + i18n。
///
/// 不加载扩展注册表 / Pinia / 搜索引擎（与 pin / screenshot 入口同范式）。
/// 剪辑会话状态全部由 ClipWindow 内模块级 reactive 持有，不跨窗口共享。
import { createApp } from 'vue'
import { initChildTheme } from '@/runtime/child-theme'
import { initI18n } from '@/runtime/i18n'
import '@/locales'
import '@ext/video/locales'
import ClipWindow from '@ext/video/windows/ClipWindow.vue'
import 'virtual:uno.css'
import '../styles/theme.css'

createApp(ClipWindow).mount('#app')
initChildTheme()
initI18n()

// 与其余窗口一致：禁用右键菜单
document.addEventListener('contextmenu', (e) => e.preventDefault())
