import { defineConfig } from '@/runtime/storage'
import type { Tool } from './logic'

/** 移除背景引擎：vision = 系统内置模型（本地、免费）；bria = 云端 RMBG-2.0（Token）。 */
export type RemoveBgEngine = 'vision' | 'bria'

export interface ImageConfig {
  defaultTool: Tool
  /** 空 = 与源文件同目录 */
  outputDir: string
  removeBgEngine: RemoveBgEngine
  /** Bria API Token（`api_token` 请求头；明文存本地扩展配置） */
  briaToken: string
  /** 云端输入最长边（px）：0 = 原始尺寸；越小上传/下载越快 */
  briaMaxEdge: number
}

export const config = defineConfig<ImageConfig>('extensions/image/config', {
  defaultTool: 'removeBg',
  outputDir: '',
  removeBgEngine: 'vision',
  briaToken: '',
  briaMaxEdge: 2048,
})
