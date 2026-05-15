import { ref } from 'vue'
import { defineAsyncComponent } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { registerModule } from '@/core/module-registry'
import type { AppModule } from '@/types/module'
import { commands, type ClipboardItem } from '@/bindings'

const ClipboardView = defineAsyncComponent(() => import('./ClipboardView.vue'))
const ClipboardSettings = defineAsyncComponent(() => import('./ClipboardSettings.vue'))
const ClipboardHeader = defineAsyncComponent(() => import('./ClipboardHeader.vue'))
const ClipboardToolbar = defineAsyncComponent(() => import('./ClipboardToolbar.vue'))

export type { ClipboardItem }

export const history = ref<ClipboardItem[]>([])
export const activeTab = ref<'all' | 'favorites'>('all')

export async function fetchClipboardHistory(
  query: string = '',
  filterFavorite: boolean = false,
) {
  try {
    const res = await commands.getClipboardHistory(
      query || null,
      filterFavorite || null,
      100,
    )
    history.value = res
  } catch (e) {
    console.error('Failed to fetch clipboard history:', e)
  }
}

const mod: AppModule = {
  id: 'clipboard',
  name: '剪贴板',
  description: '管理剪贴板记录',
  icon: 'i-ri-clipboard-line',
  keywords: [
    'clipboard',
    'copy',
    'paste',
    'history',
    '剪贴板',
    '历史',
    '复制',
    '粘贴',
  ],
  shortcut: '⌘⇧C',
  placeholder: '搜索剪贴板记录',
  order: 1,
  layout: {
    view: ClipboardView,
    header: ClipboardHeader,
  },
  settings: ClipboardSettings,
  toolbar: ClipboardToolbar,
  onInit: async () => {
    // 后台 Rust 任务负责轮询，此处无需初始化
  },
  onSearch: async (query) => {
    if (query.toLowerCase().includes('clipboard') || query.includes('剪贴板')) {
      return [{
        id: 'clipboard-module',
        title: '剪贴板历史',
        description: '打开剪贴板管理扩展',
        module: 'clipboard',
        icon: 'i-ri-clipboard-line',
        score: 100,
        data: { kind: 'module', moduleId: 'clipboard' }
      }]
    }

    try {
      const items = await commands.getClipboardHistory(query || null, null, 20)
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
    const id = result.data?.id || result.id
    if (id) {
      try {
        await invoke('paste_clipboard_item', { id })
      } catch (e) {
        console.error('Failed to paste clipboard item:', e)
      }
    }
  },
}

registerModule(mod)
