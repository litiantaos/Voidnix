/// 分屏面板独立入口：只加载 Vue + SnapPanel 组件 + 主题 CSS。
import { createApp } from 'vue'
import { initChildTheme } from '@/runtime/child-theme'
import SnapPanel from '@ext/window-manager/windows/SnapPanel.vue'
import 'virtual:uno.css'
import '../styles/theme.css'

createApp(SnapPanel).mount('#app')
initChildTheme()

document.addEventListener('contextmenu', (e) => e.preventDefault())
