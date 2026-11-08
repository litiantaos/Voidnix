import { defineConfig } from '@/runtime/storage'

/** 压缩输出目录：空 = 每次经目录选择器指定（TCC 合规，picker 授权即用户授权）。 */
export interface ArchiveConfig {
  /** 压缩默认输出目录：空 = 每次选择 */
  compressDir: string
  /** 默认压缩等级 1-9（格式与过滤开关不落盘，每次进入重置默认：zip / 垃圾过滤开 / 隐藏过滤关） */
  level: number
}

export const config = defineConfig<ArchiveConfig>('extensions/archive/config', {
  compressDir: '',
  level: 6,
})
