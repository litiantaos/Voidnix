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
      '@': resolve(__dirname, 'src'),
      '@ext': resolve(__dirname, 'extensions'),
    },
  },
  build: {
    // 主 bundle 含全部模块 View（AGENTS.md 设计决策：静态 import 首次进入零卡顿），
    // 本地 file:// 加载体积不敏感，调高阈值消除噪音。
    chunkSizeWarningLimit: 700,
    rollupOptions: {
      input: {
        main: resolve(__dirname, 'index.html'),
      },
    },
  },
}))
