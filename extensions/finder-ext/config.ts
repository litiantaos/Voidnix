import { defineConfig } from '@/runtime/storage'

/// finder-ext 扩展配置（持久化至 extensions/finder-ext/config.json）。
export const config = defineConfig('extensions/finder-ext/config', {
  /** 「用 App 打开」最近使用的应用路径（MRU，上限 3），选择器置顶直达。 */
  recentApps: [] as string[],
})

/** 记录一次「用 App 打开」使用：去重置顶，截断至上限。 */
export function rememberRecentApp(path: string) {
  const list = config.recentApps.filter((p) => p !== path)
  list.unshift(path)
  config.recentApps = list.slice(0, 3)
}
