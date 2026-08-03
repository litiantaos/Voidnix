/// 钉图窗口独立入口：只加载 Vue + PinWindow 组件 + 主题 CSS + BaseButton/BaseSlider。
import { createApp } from 'vue'
import { initChildTheme } from '@/runtime/child-theme'
import PinWindow from '@ext/screenshot/windows/PinWindow.vue'
import 'virtual:uno.css'
import '../styles/theme.css'

createApp(PinWindow).mount('#app')
initChildTheme()

document.addEventListener('contextmenu', (e) => e.preventDefault())
