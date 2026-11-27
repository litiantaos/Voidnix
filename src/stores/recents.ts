import { defineStore } from 'pinia'
import { ref } from 'vue'
import { LIMITS } from '@/runtime/constants'

/** 最近访问的一屏（扩展 + 子视图）。恢复目标记「哪一屏」+ 离开时的过滤词：
 *  视图实例状态由 KeepAlive 缓存保留（resume 激活不触发新会话重置），此处只负责导航
 *  与共享搜索框 query 的还原。 */
export interface RecentView {
  extId: string
  /** null = 扩展主视图 */
  subview: string | null
  /** 离开时的搜索 / 过滤词（恢复时还原，保持界面现场） */
  query: string
  /** 离开时扩展上报的一行内容摘要（空 = 无摘要；列表展示时回退 query） */
  summary: string
  /** 离开时刻（epoch ms），用于未来展示相对时间；当前仅排序留痕 */
  at: number
}

/** 内容摘要上限：单行、短到列表可读（超长由 UI 省略） */
const SUMMARY_MAX = 80

function clipSummary(raw: string): string {
  return raw.replace(/\s+/g, ' ').trim().slice(0, SUMMARY_MAX)
}

/// WebContent navigate 重载会清零全部 JS 单例；sessionStorage 在同一浏览会话内跨导航
/// 存活——与激活扩展恢复（voidnix.active-ext）同作用域。应用冷启动是全新会话，
/// 视图状态本来就不在，列出来是假承诺，故不落 localStorage。
const STORAGE_KEY = 'voidnix.recent-views'

function normalize(raw: unknown): RecentView | null {
  if (!raw || typeof raw !== 'object') return null
  const v = raw as Record<string, unknown>
  if (typeof v.extId !== 'string' || !v.extId) return null
  return {
    extId: v.extId,
    subview: typeof v.subview === 'string' && v.subview ? v.subview : null,
    query: typeof v.query === 'string' ? v.query : '',
    summary: typeof v.summary === 'string' ? clipSummary(v.summary) : '',
    at: typeof v.at === 'number' ? v.at : 0,
  }
}

function load(): RecentView[] {
  try {
    const parsed: unknown = JSON.parse(sessionStorage.getItem(STORAGE_KEY) ?? '[]')
    if (!Array.isArray(parsed)) return []
    return parsed
      .map(normalize)
      .filter((v): v is RecentView => v !== null)
      .slice(0, LIMITS.maxRecentViews)
  } catch {
    return []
  }
}

/** 最近访问记录：离开一屏时写入（extId + subview），去重后置顶，超出上限淘汰最旧。
 *  记录点集中在 appStore.setActiveExtension（所有离开路径的唯一收口）；内容摘要由
 *  View 经 useScreenSummary 可选上报，框架读取后随记录落库。 */
export const useRecentsStore = defineStore('recents', () => {
  const views = ref<RecentView[]>(load())

  function persist() {
    try {
      sessionStorage.setItem(STORAGE_KEY, JSON.stringify(views.value))
    } catch {
      /* 存储不可用仅损失重载恢复，静默降级 */
    }
  }

  /** 清空最近记录（自测等需要干净起点的场景消费）。 */
  function clear() {
    views.value = []
    try {
      sessionStorage.removeItem(STORAGE_KEY)
    } catch {
      /* 存储不可用仅损失持久化，静默降级 */
    }
  }

  /** 记录离开的一屏（过滤词 + 扩展上报的内容摘要）。同一屏重复离开只保留最新一条（置顶）。 */
  function record(extId: string, subview: string | null, query: string, summary = '') {
    const next = subview ?? null
    views.value = [
      { extId, subview: next, query, summary: clipSummary(summary), at: Date.now() },
      ...views.value.filter((v) => !(v.extId === extId && v.subview === next)),
    ].slice(0, LIMITS.maxRecentViews)
    persist()
  }

  return { views, record, clear }
})
