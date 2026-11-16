import { defineConfig } from '@/runtime/storage'

/** 压缩等级（主表单选择即记忆）；输出目录不落盘——默认源文件/压缩包同目录，会话内可另选。 */
export interface ArchiveConfig {
  /** 默认压缩等级 1-9（格式与过滤开关不落盘，每次进入重置默认：zip / 垃圾过滤开 / 隐藏过滤关） */
  level: number
}

export const config = defineConfig<ArchiveConfig>('extensions/archive/config', {
  level: 6,
})
