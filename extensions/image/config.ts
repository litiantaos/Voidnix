import { defineConfig } from '@/runtime/storage'
import type { Tool } from './logic'

export interface ImageConfig {
  defaultTool: Tool
  /** 空 = 与源文件同目录 */
  outputDir: string
}

export const config = defineConfig<ImageConfig>('extensions/image/config', {
  defaultTool: 'removeBg',
  outputDir: '',
})
