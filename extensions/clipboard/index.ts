import { ref, shallowRef } from 'vue'
import { defineExtension } from '@/runtime/extension-registry'
import { defineAsyncComponent } from 'vue'
import { makeToggleHandler } from '@/stores/app'
import type { ProviderResult } from '@/runtime/types'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { useAppStore } from '@/stores/app'
import { listen } from '@tauri-apps/api/event'
import { filterByQuery } from './logic'
import ClipboardSettings from './Settings.vue'

const ClipboardView = defineAsyncComponent(() => import('./View.vue'))
const ClipboardActions = defineAsyncComponent(() => import('./Actions.vue'))

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

const tabCache = new Map<string, ClipboardItem[]>()
export const loading = ref(false)
let fetchVersion = 0

let _deleteHandler: (() => void) | null = null
export function registerDeleteHandler(fn: () => void) {
  _deleteHandler = fn
}
export function triggerDelete() {
  _deleteHandler?.()
}

export async function fetchClipboardHistory(query: string = '', filterFavorite: boolean = false) {
  const tab = filterFavorite ? 'favorites' : 'all'
  const cached = tabCache.get(tab)

  if (cached) {
    history.value = filterByQuery(cached, query)
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
      history.value = filterByQuery(res, query)
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

export default defineExtension({
  meta: {
    id: 'clipboard',
    name: '剪贴板',
    description: '剪贴板历史管理',
    icon: 'i-ri-clipboard-line',
    keywords: ['clipboard', 'copy', 'paste', 'history', '剪贴板', '历史', '复制', '粘贴'],
    order: 1,
  },

  placeholder: '搜索剪贴板记录',
  mainView: () => ClipboardView,
  searchBarAccessory: () => ClipboardActions,
  settingsView: () => ClipboardSettings,
  hints: { enter: '粘贴', multiSelect: 'true', delete: '删除' },
  listOptions: { multiSelect: true },
  globalShortcuts: [
    {
      id: 'clipboard',
      default: 'CommandOrControl+Shift+C',
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
    dynamic: async (query): Promise<ProviderResult[]> => {
      if (!query.trim()) return []

      try {
        const raw = await invoke<ClipboardItem[]>(CMD.getClipboardHistory, {
          filterFavorite: null,
          limit: null,
          previewOnly: null,
        })
        const items = filterByQuery(raw, query)
        return items.map((item) => {
          let title = item.content
          if (item.content_type === 'image') {
            title = '[图片]'
          } else if (item.content_type === 'file') {
            title = '[文件] ' + item.content.split('/').pop()
          } else {
            title = item.content.substring(0, 500).replace(/\r?\n/g, ' ')
          }

          return {
            id: `clipboard-${item.id}`,
            title: title,
            description: `${item.source_app} • ${item.created_at}`,
            icon:
              item.content_type === 'image'
                ? 'i-ri-image-line'
                : item.content_type === 'file'
                  ? 'i-ri-file-line'
                  : 'i-ri-file-text-line',
            data: {
              kind: 'clipboard' as const,
              iconStyle: 'rounded',
              id: item.id,
              icon:
                item.content_type === 'image'
                  ? item.content.replace('data:image/png;base64,', '')
                  : undefined,
            },
          }
        })
      } catch {
        return []
      }
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
      }
    }
  },
})
