import { defineConfig } from 'astro/config'

// 静态输出，零 JS 优先。site 用于 OG / canonical 绝对 URL。
export default defineConfig({
  output: 'static',
  site: 'https://voidnix-lake.vercel.app',
  build: { inlineStylesheets: 'auto' },
})
