import { ref, shallowRef } from 'vue'
import { registerModule } from '@/core/module-registry'
import { asyncView } from '@/core/async-view'
import { moduleSelfResult, makeToggleHandler } from '@/core/module-helpers'
import type { AppModule } from '@/types/module'
import { commands, type ClipboardItem } from '@/bindings'
import { useAppStore } from '@/stores/app'
import { listen } from '@tauri-apps/api/event'

const ClipboardView = asyncView(() => import('./View.vue'))
const ClipboardSettings = asyncView(() => import('./Settings.vue'))
const ClipboardActions = asyncView(() => import('./Actions.vue'))

export type { ClipboardItem }

export const history = shallowRef<ClipboardItem[]>([])
export const activeTab = ref<'all' | 'favorites'>('all')

const tabCache = new Map<string, ClipboardItem[]>()
export const loading = ref(false)

let _deleteHandler: (() => void) | null = null
export function registerDeleteHandler(fn: () => void) { _deleteHandler = fn }
export function triggerDelete() { _deleteHandler?.() }

function cacheKey(tab: 'all' | 'favorites', query: string) {
  return `${tab}:${query}`
}

export async function fetchClipboardHistory(
  query: string = '',
  filterFavorite: boolean = false,
) {
  const tab = filterFavorite ? 'favorites' : 'all'
  const key = cacheKey(tab, query)

  if (tabCache.has(key)) {
    history.value = tabCache.get(key)!
    return
  }

  if (loading.value) return
  loading.value = true
  try {
    const res = await commands.getClipboardHistory(
      query || null,
      filterFavorite || null,
      null,
      true,
    )
    history.value = res
    tabCache.set(key, res)
  } catch (e) {
    console.error('Failed to fetch clipboard history:', e)
  } finally {
    loading.value = false
  }
}

export function invalidateCache() {
  tabCache.clear()
}

const mod: AppModule = {
  id: 'clipboard',
  name: '剪贴板',
  description: '管理剪贴板记录',
  icon: 'i-ri-clipboard-line',
  keywords: ['clipboard', 'copy', 'paste', 'history', '剪贴板', '历史', '复制', '粘贴'],
  shortcut: '⌘⇧C',
  placeholder: '搜索剪贴板记录',
  order: 1,
  layout: {
    view: ClipboardView,
    searchBarAccessory: ClipboardActions,
  },
  panel: ClipboardSettings,
  globalShortcuts: [
    {
      id: 'clipboard',
      default: 'CommandOrControl+Shift+C',
      onExecute: makeToggleHandler('clipboard'),
    },
  ],
  onInit: async () => {
    await fetchClipboardHistory('', false)
    await listen('cmd-backspace', () => {
      const appStore = useAppStore()
      if (appStore.activeModuleId === 'clipboard') {
        triggerDelete()
      }
    })
  },
  onActivate: async () => {
    activeTab.value = 'all'
  },
  onSearch: async (query) => {
    if (!query.trim()) return []

    if (query.toLowerCase().includes('clipboard') || query.includes('剪贴板')) {
      return [moduleSelfResult(mod)]
    }

    try {
      const items = await commands.getClipboardHistory(query || null, null, null, null)
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
          module: 'clipboard',
          icon:
            item.content_type === 'image'
              ? 'i-ri-image-line'
              : item.content_type === 'file'
                ? 'i-ri-file-line'
                : 'i-ri-file-text-line',
          score: item.score > 0 ? item.score : 500,
          data: {
            kind: 'clipboard',
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
  onExecute: async (result) => {
    const id = (result.data?.id as string) || result.id
    if (id) {
      try {
        await commands.pasteClipboardItem(id)
        invalidateCache()
      } catch (e) {
        console.error('Failed to paste clipboard item:', e)
      }
    }
  },
}

registerModule(mod)