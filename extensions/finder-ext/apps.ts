import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { CMD } from '@/commands'
import { isTauri, type RawSearchResult } from '@/utils/tauri'

/** 已安装应用条目（search_apps 元数据 + 图标合流后）。 */
export interface AppEntry {
  id: string
  name: string
  path: string
  /** base64 PNG（图标未就绪时缺省，行内字体图标兜底） */
  icon?: string
}

// ── 应用列表缓存：复用 search 扩展命令（应用枚举 + 图标提取的唯一实现）──
// 与 search/index.ts 同款失效事件（应用增删 / 图标就绪），单缓存整体失效重拉
let appCache: AppEntry[] | null = null

listen('app-cache-updated', () => {
  appCache = null
}).catch(() => {})
listen('app-icons-updated', () => {
  appCache = null
}).catch(() => {})

/** 拉取已安装应用列表：search_apps 元数据（轻）先行，get_app_icons 图标（重）按 id 合流。 */
export async function fetchApps(): Promise<AppEntry[]> {
  if (appCache) return appCache
  if (!isTauri) return []
  const raw = await invoke<RawSearchResult[]>(CMD.searchApps)
  const apps: AppEntry[] = raw.map((r) => ({
    id: r.id,
    name: r.title,
    path: r.path,
    icon: r.icon ?? undefined,
  }))
  const icons = await invoke<{ id: string; icon: string | null }[]>(CMD.getAppIcons).catch(
    () => [] as { id: string; icon: string | null }[],
  )
  const iconMap = new Map(icons.map(({ id, icon }) => [id, icon]))
  for (const a of apps) {
    const ic = iconMap.get(a.id)
    if (ic) a.icon = ic
  }
  appCache = apps
  return apps
}

/** 构建「用 App 打开」内联候选（主流做法：最近使用置顶 + 类型推荐补足）：
 *  - MRU（最近使用，跨类型）置顶——访达「打开方式」的 recent 语义
 *  - LaunchServices 类型推荐（偏好序：默认应用在前）补足至 cap
 *  - 与已安装列表 join 不上的候选（Playwright 缓存浏览器 / 系统卷投影等脏项）静默剔除
 *  纯函数（apps.test.ts 覆盖）。 */
export function buildCandidates(
  installed: AppEntry[],
  lsPaths: string[],
  recentPaths: string[],
  cap = 5,
): AppEntry[] {
  const byPath = new Map(installed.map((a) => [a.path, a]))
  const out: AppEntry[] = []
  const seen = new Set<string>()
  const push = (path: string) => {
    if (out.length >= cap || seen.has(path)) return
    const hit = byPath.get(path)
    if (!hit) return
    seen.add(path)
    out.push(hit)
  }
  for (const p of recentPaths) push(p)
  for (const p of lsPaths) push(p)
  return out
}
