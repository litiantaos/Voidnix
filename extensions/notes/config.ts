import { defineConfig } from '@/runtime/storage'

/// notes 扩展自管内容(临时保存,持久化至 extensions/notes/config.json,defineConfig 自动防抖落盘)。
export const config = defineConfig('extensions/notes/config', {
  content: '',
})
