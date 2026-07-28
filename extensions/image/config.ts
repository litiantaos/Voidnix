import { defineConfig } from '@/runtime/storage'

export interface ImageConfig {
  /** 空 = 与源文件同目录 */
  outputDir: string
}

export const config = defineConfig<ImageConfig>('extensions/image/config', {
  outputDir: '',
})
