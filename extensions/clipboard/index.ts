import { ref, shallowRef } from 'vue'
import { defineExtension } from '@/runtime/extension-registry'
import { makeToggleHandler } from '@/stores/app'
import type { ProviderResult } from '@/runtime/types'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { useAppStore } from '@/stores/app'
import { listen } from '@tauri-apps/api/event'
import { toErrorMessage } from '@/utils/format'
import {
  filterByQuery,
  filterByType,
  clipboardTitle,
  clipboardIcon,
  type ContentType,
} from './logic'
import ClipboardSettings from './Settings.vue'
import ClipboardView from './View.vue'
import ClipboardActions from './Actions.vue'

export interface ClipboardItem {
  id: string
  content: string
  content_type: string
  source_app: string
  created_at: string
  is_favorite: boolean
  score: number
  file_size: number | null
  image_width: number | null
  image_height: number | null
}

export const history = shallowRef<ClipboardItem[]>([])
export const activeTab = ref<'all' | 'favorites'>('all')
export const activeType = ref<ContentType>('all')

const tabCache = new Map<string, ClipboardItem[]>()
export const loading = ref(false)
let fetchVersion = 0

export async function fetchClipboardHistory(query: string = '', filterFavorite: boolean = false) {
  const tab = filterFavorite ? 'favorites' : 'all'
  const cached = tabCache.get(tab)

  if (cached) {
    history.value = applyFilters(cached, query)
    return
  }

  const version = ++fetchVersion
  loading.value = true
  try {
    const res = await invoke<ClipboardItem[]>(CMD.getClipboardHistory, {
      filterFavorite: filterFavorite || null,
      limit: null,
      previewOnly: true,
    })
    if (version === fetchVersion) {
      tabCache.set(tab, res)
      history.value = applyFilters(res, query)
    }
  } catch (e) {
    console.error('Failed to fetch clipboard history:', e)
  } finally {
    if (version === fetchVersion) {
      loading.value = false
    }
  }
}

export function invalidateCache() {
  tabCache.clear()
}

function applyFilters(items: ClipboardItem[], query: string): ClipboardItem[] {
  return filterByQuery(filterByType(items, activeType.value), query)
}

/** clipboard 原始记录 → 搜索结果（全局 dynamic 专用映射，复用 tabCache 避免每键 IPC）。 */
function mapClipboardResults(raw: ClipboardItem[], query: string): ProviderResult[] {
  return filterByQuery(raw, query).map((item) => ({
    id: `clipboard-${item.id}`,
    title: clipboardTitle(item),
    description: `${item.source_app} • ${item.created_at}`,
    icon: clipboardIcon(item),
    data: {
      kind: 'clipboard' as const,
      iconStyle: 'rounded',
      id: item.id,
    },
  }))
}

export default defineExtension({
  meta: {
    id: 'clipboard',
    name: '剪贴板',
    description: '剪贴板历史管理',
    icon: 'i-ri-clipboard-line',
    keywords: ['clipboard', 'copy', 'paste', 'history', '剪贴板', '历史', '复制', '粘贴'],
    order: 10,
  },

  placeholder: '搜索剪贴板记录',
  mainView: () => ClipboardView,
  searchBarAccessory: () => ClipboardActions,
  subviews: { config: () => ClipboardSettings },
  listOptions: { multiSelect: true },
  globalShortcuts: [
    {
      id: 'clipboard',
      default: 'Alt+C',
      onExecute: makeToggleHandler('clipboard'),
    },
  ],

  setup: async () => {
    await fetchClipboardHistory('', false)
    await listen('clipboard-updated', () => {
      const appStore = useAppStore()
      invalidateCache()
      fetchClipboardHistory(appStore.searchQuery, activeTab.value === 'favorites')
    })
  },

  search: {
    // 优先复用 tabCache（setup 预拉 + clipboard-updated 事件驱动 invalidateCache），
    // cache 命中同步返回零 IPC；miss 才 invoke 并回填 cache。消除每键全量拉取 + 全量打分。
    dynamic: (query): ProviderResult[] | Promise<ProviderResult[]> => {
      const q = query.trim()
      if (!q) return []

      const cached = tabCache.get('all')
      if (cached) return mapClipboardResults(cached, q)

      return (async () => {
        try {
          // M-cb2：全局搜索走 previewOnly=true，避免大图全量载入内存（每张可达数 MB）
          // 图片返回空 content + icon class；详情在 clipboard 模块 View 内按需加载
          const raw = await invoke<ClipboardItem[]>(CMD.getClipboardHistory, {
            filterFavorite: null,
            limit: null,
            previewOnly: true,
          })
          tabCache.set('all', raw)
          return mapClipboardResults(raw, q)
        } catch {
          return []
        }
      })()
    },
  },

  onExecute: async (result) => {
    const id = (result.data?.id as string) || result.id
    if (id) {
      try {
        await invoke(CMD.pasteClipboardItem, { id })
        invalidateCache()
      } catch (e) {
        console.error('Failed to paste clipboard item:', e)
        useAppStore().showStatus(toErrorMessage(e, '粘贴失败'), {
          kind: 'error',
          duration: 4000,
        })
      }
    }
  },
})
