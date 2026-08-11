/// 截图窗口独立入口：只加载 Vue + Host 组件 + 主题 CSS + i18n，
/// 不加载扩展注册表 / pinyin / markdown / 搜索引擎（省 ~500K JS）。
import { createApp } from 'vue'
import { initChildTheme } from '@/runtime/child-theme'
import { initI18n } from '@/runtime/i18n'
import '@/locales'
import '@ext/screenshot/locales'
import Host from '@ext/screenshot/windows/Host.vue'
import 'virtual:uno.css'
import '../styles/theme.css'

createApp(Host).mount('#app')
initChildTheme()
initI18n()

// 全局禁用右键菜单（与主窗口一致）
document.addEventListener('contextmenu', (e) => e.preventDefault())
