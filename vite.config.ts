import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import UnoCSS from 'unocss/vite'
import { resolve } from 'path'

const host = process.env.TAURI_DEV_HOST

export default defineConfig(async ({ command }) => ({
  plugins: [vue(), UnoCSS()],
  // 生产构建剥离 console.* / debugger：release 无 inspector，残留输出纯属死码体积与噪音
  esbuild: {
    drop: command === 'build' ? ['console', 'debugger'] : [],
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: 'ws',
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
  resolve: {
    alias: {
      '@': resolve(import.meta.dirname, 'src'),
      '@ext': resolve(import.meta.dirname, 'extensions'),
    },
  },
  build: {
    // 主 bundle 含全部模块 View（AGENTS.md 设计决策：静态 import 首次进入零卡顿），
    // 本地 file:// 加载体积不敏感，调高阈值消除噪音。
    chunkSizeWarningLimit: 700,
    // 多入口共享 CSS：所有 CSS 合并为单文件，确保子窗口（screenshot/snap-panel/pin）
    // 获得完整的 UnoCSS 原子 CSS + theme.css。UnoCSS 虚拟模块在多入口构建时仅取
    // first occurrence，默认 CSS 分割会导致非首个入口的原子类缺失。
    cssCodeSplit: false,
    rollupOptions: {
      input: {
        main: resolve(import.meta.dirname, 'index.html'),
        screenshot: resolve(import.meta.dirname, 'screenshot.html'),
        'snap-panel': resolve(import.meta.dirname, 'snap-panel.html'),
        pin: resolve(import.meta.dirname, 'pin.html'),
      },
      output: {
        // vendor 分包：跨版本缓存边界 + 按需加载。
        // vue 独立 chunk 供所有入口共享；pinia 仅 main 入口消费，不混入 vendor
        // 以免子窗口（screenshot/snap-panel/pin）被迫加载 Pinia 运行时。
        manualChunks(id) {
          if (!id.includes('node_modules')) return
          if (id.includes('pinyin-pro')) return 'pinyin'
          if (id.includes('marked') || id.includes('dompurify')) return 'markdown'
          if (id.includes('vue')) return 'vendor'
        },
      },
    },
  },
}))
