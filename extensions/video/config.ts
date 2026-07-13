import { defineConfig } from '@/runtime/storage'
import type { OutputFormat, Quality, Scale, VideoMode } from './logic'

export interface VideoConfig {
  defaultMode: VideoMode
  defaultQuality: Quality
  defaultFormat: OutputFormat
  defaultScale: Scale
  /** 空 = 与源同目录 */
  outputDir: string
  preferHardware: boolean
}

export const config = defineConfig<VideoConfig>('extensions/video/config', {
  defaultMode: 'compress',
  defaultQuality: 'balanced',
  defaultFormat: 'mp4',
  defaultScale: 'original',
  outputDir: '',
  preferHardware: true,
})
